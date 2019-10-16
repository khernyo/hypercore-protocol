use std::fmt::{Debug, Error, Formatter};

use enum_as_inner::EnumAsInner;
use slog::{o, trace, Drain, Logger};

use crate::protocol::{Channel, DiscoveryKey, Key, Message, MessageType};
use crate::schema;
use crate::wire_format::{self, write_msg};

pub trait FeedStream {
    fn _push(&mut self, bytes: &[u8]);
    fn _onhandshake(&mut self, handshake: &schema::Handshake);
}

pub struct Feed<FS: FeedStream, E: FeedEventEmitter> {
    log: Logger,

    pub(crate) key: Option<Key>,
    pub(crate) discovery_key: Option<DiscoveryKey>,
    stream: FS,
    emitter: E,

    pub(crate) id: Option<Channel>,
    pub(crate) remote_id: Option<Channel>,
    header: (),
    header_length: (),
    closed: bool,

    pub(crate) _buffer: Option<Vec<Message>>,
}

impl<FS: FeedStream, E: FeedEventEmitter> Debug for Feed<FS, E> {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        f.debug_struct("Feed")
            .field("key", &self.key)
            .field("discovery_key", &self.discovery_key)
            .field("id", &self.id)
            .field("remote_id", &self.remote_id)
            .field("header", &self.header)
            .field("header_length", &self.header_length)
            .field("closed", &self.closed)
            .field("_buffer", &self._buffer)
            .finish()
    }
}

impl<FS: FeedStream, E: FeedEventEmitter> Feed<FS, E> {
    pub(crate) fn new<L: Into<Option<slog::Logger>>>(
        logger: L,
        stream: FS,
        emitter: E,
    ) -> Feed<FS, E> {
        let log = logger
            .into()
            .unwrap_or_else(|| slog::Logger::root(slog_stdlog::StdLog.fuse(), o!()));
        Feed {
            log,
            key: None,
            discovery_key: None,
            stream,
            emitter,
            id: None,
            remote_id: None,
            header: (),
            header_length: (),
            closed: false,
            _buffer: Some(Vec::new()),
        }
    }

    pub(crate) fn handshake(&mut self, handshake: schema::Handshake) {
        slog::trace!(self.log, "Sending handshake: {:?}", handshake);
        let bytes = write_msg(self.id.unwrap(), &Message::Handshake(handshake)).unwrap();
        self.stream._push(&bytes);
    }

    pub(crate) fn data(&mut self, data: schema::Data) {
        slog::trace!(self.log, "Sending data: {:?}", data);
        let bytes = write_msg(self.id.unwrap(), &Message::Data(data)).unwrap();
        self.stream._push(&bytes);
    }

    pub(crate) fn _onclose(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;

        unimplemented!()
        //        if !self.stream.destroyed {
        //            self.close();
        //            if self.remote_id.is_some() {
        //                self.stream._remoteFeeds[this.remoteId] = null;
        //            }
        //            let dk = self.discovery_key.as_ref().unwrap();
        //            if self.stream._feeds[dk] == self {
        //                delete this.stream._feeds[hex];
        //            }
        //        }

        //        self.emit('close');
    }

    pub(crate) fn _resume(&mut self) {
        unimplemented!()
    }

    pub(crate) fn _onextension(&self, bytes: &[u8], start: usize, end: usize) {
        unimplemented!()
    }

    pub(crate) fn _onmessage(
        &mut self,
        r#type: MessageType,
        bytes: &[u8],
        start: usize,
        end: usize,
    ) {
        trace!(
            self.log,
            "_onmessage({:?}, {:?}, {}, {})",
            r#type,
            bytes,
            start,
            end
        );
        let message = wire_format::read_msg2(r#type, &bytes[start..end]).unwrap();
        assert_eq!(message.r#type(), r#type);

        if self.closed {
            return;
        }

        if let Message::Handshake(ref handshake) = message {
            return self.stream._onhandshake(handshake);
        }

        if self._buffer.is_none() {
            self.emitter.emit(FeedEvent::Message(message));
            return;
        }

        if self._buffer.as_ref().unwrap().len() > 16 {
            return self.destroy("Remote sent too many messages on an unopened feed");
        }

        self._buffer.as_mut().unwrap().push(message);
    }

    fn destroy(&mut self, err: &str) {
        unimplemented!()
    }
}

#[derive(Clone, Debug, PartialEq, EnumAsInner)]
pub enum FeedEvent {
    Feed(DiscoveryKey),
    Handshake,

    // TODO not all message types will be emitted, and it should be reflected. (Handshake and Feed are not emitted, maybe others, too)
    Message(Message),
}
pub trait FeedEventEmitter {
    fn emit(&mut self, event: FeedEvent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use data_encoding::HEXLOWER;

    struct TestStream<'a>(&'a mut Vec<Vec<u8>>);
    impl<'a> FeedStream for TestStream<'a> {
        fn _push(&mut self, bytes: &[u8]) {
            self.0.push(bytes.to_owned());
        }

        fn _onhandshake(&mut self, handshake: &schema::Handshake) {
            unimplemented!()
        }
    }

    struct TestEmitter<'a>(&'a mut Vec<FeedEvent>);
    impl<'a> FeedEventEmitter for TestEmitter<'a> {
        fn emit(&mut self, event: FeedEvent) {
            self.0.push(event);
        }
    }

    #[test]
    fn send_handshake() {
        let mut stream_bytes = Vec::new();
        let mut events = Vec::new();
        let mut feed = Feed::new(
            None,
            TestStream(&mut stream_bytes),
            TestEmitter(&mut events),
        );
        feed.id = Some(Channel(0));
        let mut handshake = schema::Handshake::new();
        handshake.set_id(b"foo"[..].into());
        handshake.set_live(true);
        handshake.set_userData(b"bar"[..].into());
        handshake.set_extensions(["baz".to_owned()][..].into());
        handshake.set_ack(true);
        feed.handshake(handshake);
        assert_eq!(
            stream_bytes
                .iter()
                .map(|bytes| HEXLOWER.encode(&bytes))
                .collect::<Vec<_>>(),
            vec!["14010a03666f6f10011a03626172220362617a2801"]
        );
    }
}
