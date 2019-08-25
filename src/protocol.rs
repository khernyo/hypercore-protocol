use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::rc::Rc;

use integer_encoding::VarInt;
use protobuf::parse_from_bytes;
use slog::{o, trace, Drain, Logger};
use sodiumoxide::crypto::generichash;

use crate::crypto_stream::{crypto_stream_xor_instance, Xor};
use crate::feed::{Feed, FeedEvent, FeedEventEmitter, FeedStream};
use crate::schema;
use crate::wire_format;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct Channel(pub(crate) u8);

impl Channel {
    pub(crate) const MAX_CHANNELS: usize = 128;
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum MessageType {
    Feed = 0,
    Handshake = 1,
    Info = 2,
    Have = 3,
    Unhave = 4,
    Want = 5,
    Unwant = 6,
    Request = 7,
    Cancel = 8,
    Data = 9,
    Extension = 15,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct Header {
    pub(crate) channel: Channel,
    pub(crate) message_type: MessageType,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Feed(schema::Feed),
    Handshake(schema::Handshake),
    Info(schema::Info),
    Have(schema::Have),
    Unhave(schema::Unhave),
    Want(schema::Want),
    Unwant(schema::Unwant),
    Request(schema::Request),
    Cancel(schema::Cancel),
    Data(schema::Data),
    Extension(Vec<u8>),
}

impl Message {
    pub(crate) fn r#type(&self) -> MessageType {
        match self {
            Message::Feed(_) => MessageType::Feed,
            Message::Handshake(_) => MessageType::Handshake,
            Message::Info(_) => MessageType::Info,
            Message::Have(_) => MessageType::Have,
            Message::Unhave(_) => MessageType::Unhave,
            Message::Want(_) => MessageType::Want,
            Message::Unwant(_) => MessageType::Unwant,
            Message::Request(_) => MessageType::Request,
            Message::Cancel(_) => MessageType::Cancel,
            Message::Data(_) => MessageType::Data,
            Message::Extension(_) => MessageType::Extension,
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Key(pub [u8; 32]);

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct DiscoveryKey([u8; 32]);

impl TryFrom<&[u8]> for DiscoveryKey {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let mut bytes = [0u8; 32];
        if value.len() != bytes.len() {
            Err(())
        } else {
            bytes.copy_from_slice(&value);
            Ok(DiscoveryKey(bytes))
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
struct Nonce([u8; 24]);

impl Nonce {
    fn new() -> Nonce {
        let mut bytes = [0; 24];
        random_bytes_into(&mut bytes);
        Nonce(bytes)
    }
}

impl TryFrom<&[u8]> for Nonce {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let mut bytes = [0u8; 24];
        if value.len() != bytes.len() {
            Err(())
        } else {
            bytes.copy_from_slice(&value);
            Ok(Nonce(bytes))
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Id([u8; 32]);

impl TryFrom<&[u8]> for Id {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let mut bytes = [0u8; 32];
        if value.len() != bytes.len() {
            Err(())
        } else {
            bytes.copy_from_slice(&value);
            Ok(Id(bytes))
        }
    }
}

pub trait Stream {
    fn _push(&mut self, bytes: &mut [u8]);
}

// encoding length of 8*1024*1024
const VARINT_8M_ENCODING_LENGTH: usize = 4;

pub struct Protocol<E: FeedEventEmitter, S: Stream> {
    log: Logger,

    stream: Rc<RefCell<S>>,
    emitter: Rc<RefCell<E>>,

    id: Id,
    live: bool,
    ack: bool,
    user_data: Option<Vec<u8>>,
    remote_id: Rc<RefCell<Option<Id>>>,
    remote_live: Rc<Cell<Option<bool>>>,
    remote_ack: Rc<Cell<Option<bool>>>,
    remote_user_data: Rc<RefCell<Option<Vec<u8>>>>,

    destroyed: Rc<Cell<bool>>,
    encrypted: bool,
    key: Option<Key>,
    discovery_key: Option<DiscoveryKey>,
    remote_discovery_key: Option<DiscoveryKey>,
    feeds: Vec<Rc<RefCell<Feed<FeedStreamHack<E, S>, FeedEventEmitterImpl>>>>,
    extensions: Rc<RefCell<Vec<String>>>,
    remote_extensions: Rc<RefCell<Vec<Option<usize>>>>,
    max_feeds: usize,

    _local_feeds: Vec<Rc<RefCell<Feed<FeedStreamHack<E, S>, FeedEventEmitterImpl>>>>,
    _remote_feeds: Vec<Option<Rc<RefCell<Feed<FeedStreamHack<E, S>, FeedEventEmitterImpl>>>>>,
    _feeds: HashMap<DiscoveryKey, Rc<RefCell<Feed<FeedStreamHack<E, S>, FeedEventEmitterImpl>>>>,

    _nonce: Option<Nonce>,
    _remote_nonce: Option<Nonce>,
    _xor: Rc<RefCell<Option<Xor>>>,
    _remote_xor: Option<Xor>,
    _needs_key: bool,
    _length: [u8; VARINT_8M_ENCODING_LENGTH],
    _missing: usize,
    _buf: Option<Vec<u8>>,
    _pointer: usize,
    _data: Option<Vec<u8>>,
    _start: usize,
    _keep_alive: Rc<Cell<u8>>,
    _remote_keep_alive: u8,
}

#[derive(Clone, Debug)]
pub struct ProtocolOpts {
    pub id: Option<Id>,
    pub live: Option<bool>,
    pub user_data: Option<Vec<u8>>,
    pub ack: Option<bool>,
    pub encrypted: Option<bool>,
    pub extensions: Option<Vec<String>>,
}

impl ProtocolOpts {
    fn new() -> Self {
        ProtocolOpts::default()
    }
}

impl Default for ProtocolOpts {
    fn default() -> Self {
        ProtocolOpts {
            id: None,
            live: None,
            user_data: None,
            ack: None,
            encrypted: None,
            extensions: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FeedOptions {
    pub discovery_key: Option<DiscoveryKey>,
}

impl<E: FeedEventEmitter, S: Stream> Protocol<E, S> {
    pub fn new<L: Into<Option<Logger>>>(
        logger: L,
        emitter: E,
        stream: S,
        opts: &ProtocolOpts,
    ) -> Protocol<E, S> {
        let log = logger
            .into()
            .unwrap_or_else(|| Logger::root(slog_stdlog::StdLog.fuse(), o!()));
        trace!(log, "Protocol::new({:?})", opts);
        debug_assert_eq!(
            VARINT_8M_ENCODING_LENGTH,
            VarInt::required_space(8 * 1024 * 1024)
        );

        Protocol {
            log,

            stream: Rc::new(RefCell::new(stream)),
            emitter: Rc::new(RefCell::new(emitter)),

            id: opts.id.clone().unwrap_or_else(random_id),
            live: opts.live.unwrap_or(false),
            ack: opts.ack.unwrap_or(false),
            user_data: opts.user_data.clone(),
            remote_id: Rc::new(RefCell::new(None)),
            remote_live: Rc::new(Cell::new(None)),
            remote_ack: Rc::new(Cell::new(None)),
            remote_user_data: Rc::new(RefCell::new(None)),

            destroyed: Rc::new(Cell::new(false)),
            encrypted: opts.encrypted.unwrap_or(true),
            key: None,
            discovery_key: None,
            remote_discovery_key: None,
            feeds: Vec::new(),
            extensions: Rc::new(RefCell::new(opts.extensions.clone().unwrap_or_default())),
            remote_extensions: Rc::new(RefCell::new(vec![])),
            max_feeds: 256,

            _local_feeds: Vec::new(),
            _remote_feeds: Vec::new(),
            _feeds: HashMap::new(),

            _nonce: None,
            _remote_nonce: None,
            _xor: Rc::new(RefCell::new(None)),
            _remote_xor: None,
            _needs_key: false,
            _length: [0u8; VARINT_8M_ENCODING_LENGTH],
            _missing: 0,
            _buf: None,
            _pointer: 0,
            _data: None,
            _start: 0,
            _keep_alive: Rc::new(Cell::new(0)),
            _remote_keep_alive: 0,
        }
    }

    pub(crate) fn has(&self, key: &Key) -> bool {
        self._feeds.contains_key(&discovery_key(&key.0))
    }

    pub fn feed(
        &mut self,
        key: &Key,
        opts: FeedOptions,
    ) -> Option<Rc<RefCell<Feed<FeedStreamHack<E, S>, FeedEventEmitterImpl>>>> {
        trace!(self.log, "Protocol::feed({:?})", opts);
        if self.destroyed.get() {
            return None;
        }

        let dk = opts.discovery_key.unwrap_or_else(|| discovery_key(&key.0));
        trace!(self.log, "Protocol::feed: {:?}", dk);
        let ch = self._feed(&dk);

        if ch.borrow().id.is_some() {
            return Some(ch.clone());
        }

        if self._local_feeds.len() >= self.max_feeds {
            self._too_many_feeds();
            return None;
        }

        let id = self._local_feeds.len();
        ch.borrow_mut().id = Some(Channel(id as u8));
        self._local_feeds.push(ch.clone());
        ch.borrow_mut().key = Some(key.clone());
        ch.borrow_mut().discovery_key = Some(dk.clone());

        self.feeds.push(ch.clone());

        let first = self.key.is_none();
        let mut feed = schema::Feed::new();
        feed.set_discoveryKey(Vec::from(&dk.0[..]));

        trace!(self.log, "Protocol::feed: first: {}", first);
        if first {
            self.key = Some(key.clone());
            self.discovery_key = Some(dk.clone());

            if !self._same_key() {
                trace!(self.log, "Protocol::feed: not same key");
                return None;
            }

            trace!(self.log, "Protocol::feed: encrypted: {}", self.encrypted);
            if self.encrypted {
                let nonce = Nonce::new();
                trace!(self.log, "Protocol::feed: nonce: {:?}", nonce);
                self._nonce = Some(nonce.clone());
                feed.set_nonce(Vec::from(nonce.0.as_ref()));

                trace!(self.log, "Protocol::feed: key: {:?}", self.key);
                *self._xor.borrow_mut() = Some(crypto_stream_xor_instance(
                    &self._nonce.as_ref().unwrap().0,
                    &self.key.as_ref().unwrap().0,
                ));
                trace!(
                    self.log,
                    "Protocol::feed: remote_nonce: {:?}",
                    self._remote_nonce
                );
                if let Some(ref remote_nonce) = self._remote_nonce {
                    self._remote_xor = Some(crypto_stream_xor_instance(
                        &remote_nonce.0,
                        &self.key.as_ref().unwrap().0,
                    ));
                }
            }

            trace!(self.log, "Protocol::feed: needs_key: {}", self._needs_key);
            if self._needs_key {
                self._needs_key = false;
                self._resume();
            }
        }

        let mut r#box = encode_feed(feed.clone(), ch.borrow().id.unwrap());
        if !feed.has_nonce() && self.encrypted {
            self._xor
                .borrow_mut()
                .as_mut()
                .unwrap()
                .update(&r#box.clone(), &mut r#box);
        }
        self._keep_alive.set(0);
        self.push(&mut r#box);

        if self.destroyed.get() {
            return None;
        }

        if first {
            let mut handshake = schema::Handshake::new();
            handshake.set_id(self.id.0[..].into());
            handshake.set_live(self.live);
            if let Some(ref user_data) = self.user_data {
                handshake.set_userData(user_data.clone())
            }
            handshake.set_extensions(self.extensions.borrow()[..].into());
            handshake.set_ack(self.ack);

            ch.borrow_mut().handshake(handshake);
        }

        if !ch.borrow()._buffer.as_ref().unwrap().is_empty() {
            ch.borrow_mut()._resume();
        } else {
            ch.borrow_mut()._buffer = None
        }

        Some(ch.clone())
    }

    pub fn push(&mut self, bytes: &mut [u8]) {
        self.stream.borrow_mut()._push(bytes);
    }

    fn _resume(&mut self) {
        // Note: the nodejs implementation runs this function on `process.nextTick`. Is
        //  it really necessary?

        trace!(
            self.log,
            "_resume(): data: {:?} start:{}",
            self._data,
            self._start
        );

        if let Some(mut data) = self._data.take() {
            let start = self._start;
            self._start = 0;

            //        let cb = self._cb;
            //        self._cb = null;

            self._parse(&mut data, start);
        }
    }

    fn destroy(&mut self, error: Option<&str>) {
        if self.destroyed.get() {
            return;
        }
        self.destroyed.set(true);
        if let Some(error) = error {
            //            this.emit('error', err);
        }
        self._close();
        //        this.emit('close');
    }

    fn _close(&mut self) {
        //        clearInterval(this._interval);

        let feeds = std::mem::replace(&mut self._feeds, HashMap::new());
        for (_, feed) in feeds {
            feed.borrow_mut()._onclose();
        }

        *self._xor.borrow_mut() = None;
    }

    pub fn _write(&mut self, bytes: &mut [u8]) {
        self._remote_keep_alive = 0;
        self._parse(bytes, 0)
    }

    fn _feed(
        &mut self,
        dk: &DiscoveryKey,
    ) -> Rc<RefCell<Feed<FeedStreamHack<E, S>, FeedEventEmitterImpl>>> {
        if let Some(ch) = self._feeds.get_mut(dk) {
            return ch.clone();
        }
        let ch = Feed::new(
            self.log.clone(),
            FeedStreamHack::new(self),
            FeedEventEmitterImpl::new(self),
        );
        self._feeds.insert(dk.clone(), Rc::new(RefCell::new(ch)));
        self._feeds.get_mut(dk).unwrap().clone()
    }

    fn _onopen(&mut self, id: Channel, bytes: &[u8], start: usize, end: usize) {
        trace!(
            self.log,
            "onopen({:?}, {:?}, {}, {})",
            id,
            bytes,
            start,
            end
        );
        let feed = decode_feed(&self.log, bytes, start, end);
        trace!(self.log, "onopen: feed: {:?}", feed);

        let feed = match feed {
            Some(feed) => feed,
            None => return self._bad_feed(),
        };

        let dk: DiscoveryKey = feed.get_discoveryKey().try_into().unwrap();
        trace!(self.log, "onopen: dk: {:?}", dk);
        trace!(
            self.log,
            "onopen: remote_discovery_key: {:?}",
            self.remote_discovery_key
        );
        if self.remote_discovery_key.is_none() {
            self.remote_discovery_key = Some(dk.clone());
            trace!(self.log, "onopen: rdk: {:?}", self.remote_discovery_key);
            if !self._same_key() {
                return;
            }

            trace!(
                self.log,
                "onopen: encrypted: {}, remote_nonce: {:?}",
                self.encrypted,
                self._remote_nonce
            );
            if self.encrypted && self._remote_nonce.is_none() {
                if !feed.has_nonce() {
                    self.destroy(Some("Remote did not include a nonce"));
                    return;
                }
                self._remote_nonce = Some(feed.get_nonce().try_into().expect("Invalid nonce"));
            }

            trace!(
                self.log,
                "onopen: encrypted: {}, key: {:?}, remote_xor: {:?}",
                self.encrypted,
                self.key,
                self._remote_xor.is_some()
            );
            if self.encrypted && self.key.is_some() && self._remote_xor.is_none() {
                self._remote_xor = Some(crypto_stream_xor_instance(
                    &self._remote_nonce.as_ref().unwrap().0,
                    &self.key.as_ref().unwrap().0,
                ));
            }
            trace!(
                self.log,
                "onopen: remote_xor: {}",
                self._remote_xor.is_some()
            );
        }

        self._remote_feeds[id.0 as usize] = Some(self._feed(&dk));
        self._remote_feeds[id.0 as usize]
            .as_ref()
            .unwrap()
            .borrow_mut()
            .remote_id = Some(id);

        //        self.emit("feed", feed.discoveryKey);
    }

    fn _onmessage(&mut self, bytes: &[u8], mut start: usize, end: usize) {
        // TODO Use wire_format::read_msg for parsing the message
        trace!(self.log, "_onmessage({:?}, {}, {})", bytes, start, end);
        if end - start < 2 {
            return;
        }

        let Header {
            channel: id,
            message_type: r#type,
        } = match decode_header(&self.log, bytes, &mut start) {
            Some(h) => h,
            None => return self.destroy(Some("Remote sent invalid header")),
        };

        // FIXME this is always false, use `Result` to handle errors
        if id.0 as usize >= self.max_feeds {
            return self._too_many_feeds();
        }
        if self._remote_feeds.len() <= id.0 as usize {
            self._remote_feeds.resize(id.0 as usize + 1, None);
        }
        assert_eq!(self._remote_feeds.len(), id.0 as usize + 1);
        let ch = &mut self._remote_feeds[id.0 as usize];

        if r#type == MessageType::Feed {
            if let Some(ref mut ch) = ch {
                ch.borrow_mut()._onclose();
            }
            return self._onopen(id, bytes, start, end);
        }

        if let Some(ch) = ch {
            trace!(self.log, "ch: {:?}", ch);
            if r#type == MessageType::Extension {
                return ch.borrow()._onextension(bytes, start, end);
            }
            ch.borrow_mut()._onmessage(r#type, bytes, start, end);
        } else {
            self._bad_feed()
        }
    }

    fn _parse(&mut self, mut bytes: &mut [u8], mut start: usize) {
        trace!(self.log, "_parse({:?}, {})", bytes, start);
        let decrypted = self._remote_xor.is_some();
        trace!(self.log, "decrypted: {}", decrypted);

        if start > 0 {
            bytes = &mut bytes[start..];
            start = 0;
        }

        trace!(self.log, "remote_xor: {:?}", self._remote_xor.is_some());
        if let Some(ref mut remote_xor) = self._remote_xor {
            remote_xor.update(&bytes.to_owned(), bytes)
        }

        while start < bytes.len() && !self.destroyed.get() {
            trace!(self.log, "missing: {}", self._missing);
            if self._missing > 0 {
                start = self._parse_message(bytes, start);
            } else {
                start = self._parse_length(bytes, start);
            }

            trace!(self.log, "needs_key: {}", self._needs_key);
            if self._needs_key {
                self._data = Some(bytes.to_owned());
                self._start = start;
                // self._cb = cb
                trace!(self.log, "Exiting _parse: needs key");
                return;
            }

            trace!(
                self.log,
                "decrypted: {}, remote_xor: {:?}",
                decrypted,
                self._remote_xor.is_some()
            );
            if !decrypted && self._remote_xor.is_some() {
                self._parse(bytes, start);
                trace!(
                    self.log,
                    "Exiting _parse: !decrypted && remote_xor.is_some()"
                );
                return;
            }
        }
        trace!(self.log, "Exiting _parse");

        // cb()
    }

    fn _parse_message(&mut self, bytes: &[u8], mut start: usize) -> usize {
        trace!(self.log, "_parse_message({:?}, {})", bytes, start);
        let mut end = start + self._missing as usize;

        if end <= bytes.len() {
            let ret = end;

            let bytes: Cow<[u8]> = if let Some(mut buf) = self._buf.take() {
                buf[self._pointer..].copy_from_slice(&bytes[start..]);
                start = 0;
                end = bytes.len();
                buf.into()
            } else {
                bytes.into()
            };

            self._missing = 0;
            self._pointer = 0;
            if self.encrypted && self.key.is_none() {
                self._needs_key = true;
            }
            self._onmessage(&bytes, start, end);

            return ret;
        }

        if self._buf.is_none() {
            self._buf = Some(vec![0u8; self._missing]);
            self._pointer = 0;
        }

        let rem = bytes.len() - start;

        self._buf.as_mut().unwrap()[self._pointer..(self._pointer + rem)]
            .copy_from_slice(&bytes[start..]);
        self._pointer += rem;
        self._missing -= rem;

        bytes.len()
    }

    fn _parse_length(&mut self, bytes: &[u8], mut start: usize) -> usize {
        while self._missing == 0 && start < bytes.len() {
            let byte = bytes[start];
            start += 1;
            self._length[self._pointer] = byte;
            self._pointer += 1;

            if byte & 0x80 == 0 {
                let (length, _) = VarInt::decode_var(&self._length);
                self._missing = length;
                self._pointer = 0;
                if self._missing > 8 * 1024 * 1024 {
                    return self._too_big(bytes.len());
                }
                return start;
            }

            if self._pointer >= self._length.len() {
                return self._too_big(bytes.len());
            }
        }

        start
    }

    fn _same_key(&mut self) -> bool {
        trace!(self.log, "Same key:");
        if !self.encrypted {
            trace!(self.log, "Same key: not encrypted");
            return true;
        }
        trace!(
            self.log,
            "Same key: {:?}",
            (&self.discovery_key, &self.remote_discovery_key)
        );
        match (&self.discovery_key, &self.remote_discovery_key) {
            (None, None) | (None, _) | (_, None) => {
                trace!(self.log, "  Same key: missing");
                true
            }
            (Some(ref dk), Some(ref rdk)) if dk == rdk => {
                trace!(self.log, " Same key: equal");
                true
            }
            _ => {
                trace!(self.log, "  Same key: Nope!");
                self.destroy(Some("First shared hypercore must be the same"));
                false
            }
        }
        //        trace!(
        //            self.log,
        //            "  Same key: {:?}",
        //            self.discovery_key.is_none() || self.remote_discovery_key.is_none()
        //        );
        //        if self.discovery_key.is_none() || self.remote_discovery_key.is_none() {
        //            return true;
        //        }
        //        trace!(
        //            self.log,
        //            "  Same key: {:?}",
        //            self.remote_discovery_key == self.discovery_key
        //        );
        //        if self.remote_discovery_key == self.discovery_key {
        //            return true;
        //        }
        //
        //        trace!(self.log, "  Same key: Nope!");
        //        self.destroy(Some("First shared hypercore must be the same"));
        //        false
    }

    fn _too_many_feeds(&self) -> ! {
        unimplemented!()
    }

    fn _too_big(&self, len: usize) -> ! {
        unimplemented!()
    }

    fn _bad_feed(&self) -> ! {
        unimplemented!()
    }

    fn emit(&self) -> ! {
        unimplemented!()
    }
}

pub struct FeedStreamHack<E: FeedEventEmitter, S: Stream> {
    stream: Rc<RefCell<S>>,
    emitter: Rc<RefCell<E>>,

    extensions: Rc<RefCell<Vec<String>>>,

    remote_id: Rc<RefCell<Option<Id>>>,
    remote_live: Rc<Cell<Option<bool>>>,
    remote_ack: Rc<Cell<Option<bool>>>,
    remote_user_data: Rc<RefCell<Option<Vec<u8>>>>,
    remote_extensions: Rc<RefCell<Vec<Option<usize>>>>,

    destroyed: Rc<Cell<bool>>,

    _xor: Rc<RefCell<Option<Xor>>>,
    _keep_alive: Rc<Cell<u8>>,
}
impl<E: FeedEventEmitter, S: Stream> FeedStreamHack<E, S> {
    fn new(protocol: &Protocol<E, S>) -> Self {
        FeedStreamHack {
            stream: protocol.stream.clone(),
            emitter: protocol.emitter.clone(),

            extensions: protocol.extensions.clone(),

            remote_id: protocol.remote_id.clone(),
            remote_live: protocol.remote_live.clone(),
            remote_ack: protocol.remote_ack.clone(),
            remote_user_data: protocol.remote_user_data.clone(),
            remote_extensions: protocol.remote_extensions.clone(),

            destroyed: protocol.destroyed.clone(),

            _xor: protocol._xor.clone(),
            _keep_alive: protocol._keep_alive.clone(),
        }
    }
}
impl<E: FeedEventEmitter, S: Stream> FeedStream for FeedStreamHack<E, S> {
    fn _push(&mut self, bytes: &[u8]) {
        log::trace!("FeedStreamHack::_push({:?})", bytes);
        if self.destroyed.get() {
            return;
        }
        self._keep_alive.set(0);

        let mut buf = vec![0u8; bytes.len()];
        if let Some(xor) = self._xor.borrow_mut().as_mut() {
            xor.update(bytes, &mut buf);
        }

        self.stream.borrow_mut()._push(&mut buf);
    }

    fn _onhandshake(&mut self, hs: &schema::Handshake) {
        log::trace!("FeedStreamHack::_onhandshake({:?})", hs);
        if self.remote_id.borrow().is_some() {
            return;
        }

        *self.remote_id.borrow_mut() = Some(if hs.has_id() {
            hs.get_id().try_into().unwrap()
        } else {
            random_id()
        });
        self.remote_live.set(if hs.has_live() {
            Some(hs.get_live())
        } else {
            None
        });
        *self.remote_user_data.borrow_mut() = if hs.has_userData() {
            Some(hs.get_userData().into())
        } else {
            None
        };
        *self.remote_extensions.borrow_mut() =
            sorted_index_of(&self.extensions.borrow(), hs.get_extensions());
        self.remote_ack.set(if hs.has_ack() {
            Some(hs.get_ack())
        } else {
            None
        });

        self.emitter.borrow_mut().emit(FeedEvent::Handshake);
    }
}

// https://github.com/mafintosh/sorted-indexof
fn sorted_index_of<T: Ord>(haystack: &[T], needles: &[T]) -> Vec<Option<usize>> {
    needles
        .iter()
        .map(|s| haystack.binary_search(s).ok())
        .collect()
}

pub struct FeedEventEmitterImpl;
impl FeedEventEmitterImpl {
    fn new<E: FeedEventEmitter, S: Stream>(protocol: &Protocol<E, S>) -> Self {
        FeedEventEmitterImpl
    }
}
impl FeedEventEmitter for FeedEventEmitterImpl {
    fn emit(&mut self, event: FeedEvent) {
        unimplemented!()
    }
}

fn random_id() -> Id {
    let mut id = [0u8; 32];
    random_bytes_into(&mut id);
    Id(id)
}

fn decode_header(log: &Logger, bytes: &[u8], start: &mut usize) -> Option<Header> {
    trace!(log, "decode_header {:?} {:?}", bytes, start);
    let (value, read_bytes) = VarInt::decode_var(&bytes[*start..]);

    // Why is 0xffff an error?
    let result = if value == 0xffff {
        None
    } else {
        *start += read_bytes;
        Some(wire_format::decode_header(value))
    };
    trace!(log, "decode_header -> {:?}", result);
    result
}

fn decode_feed(log: &Logger, bytes: &[u8], start: usize, end: usize) -> Option<schema::Feed> {
    trace!(log, "decode_feed {:?} {:?} {:?}", bytes, start, end);
    let feed = parse_from_bytes::<schema::Feed>(&bytes[start..end]).ok()?;
    trace!(log, "decode_feed feed: {:?}", feed);
    let invalid_dk = feed.get_discoveryKey().len() != 32;
    let invalid_nonce = feed.has_nonce() && feed.get_nonce().len() != 24;
    let result = if invalid_dk || invalid_nonce {
        None
    } else {
        Some(feed)
    };
    trace!(log, "decode_feed -> {:?}", result);
    result
}

fn encode_feed(feed: schema::Feed, channel: Channel) -> Vec<u8> {
    wire_format::write_msg(channel, &Message::Feed(feed)).unwrap()
}

fn discovery_key(key: &[u8]) -> DiscoveryKey {
    let mut hasher = generichash::State::new(32, Some(key)).unwrap();
    hasher.update(b"hypercore").unwrap();
    let digest = hasher.finalize().unwrap();
    let mut result = DiscoveryKey([0u8; 32]);
    result.0[..].clone_from_slice(digest.as_ref());
    result
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    random_bytes_into(&mut buf);
    buf
}

const NOT_RANDOM_BYTES: Option<[u8; 1024]> = None;

fn random_bytes_into(buf: &mut [u8]) {
    if let Some(bs) = NOT_RANDOM_BYTES {
        buf.copy_from_slice(&bs[..buf.len()]);
    } else {
        // TODO init sodiumoxide somewhere else
        sodiumoxide::init().unwrap();
        sodiumoxide::randombytes::randombytes_into(buf);
    }
}

#[cfg(test)]
mod tests {
    use data_encoding::HEXUPPER;

    use super::*;

    #[test]
    fn test_discovery_key() {
        assert_eq!(
            HEXUPPER.encode(&discovery_key(b"01234567890123456789012345678901").0),
            "103E9C9562455F70DFE3F3F9F1DC0CF8548D72D6C4B3C5AC1B44EAEFDB6F7E65"
        );
    }

    #[test]
    fn test_encode_feed() {
        let mut feed = schema::Feed::new();
        feed.set_discoveryKey(b"01234567890123456789012345678901".to_vec());
        assert_eq!(
            HEXUPPER.encode(&encode_feed(feed, Channel(42))),
            "24A0050A203031323334353637383930313233343536373839303132333435363738393031"
        )
    }

    #[test]
    fn test_sorted_index_of() {
        assert_eq!(
            sorted_index_of(&["b", "c", "d", "e", "f"], &["a", "b", "c", "f", "g", "h"]),
            vec![None, Some(0), Some(1), Some(4), None, None]
        );
    }
}
