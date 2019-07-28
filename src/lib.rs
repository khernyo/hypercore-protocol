use std::collections::HashMap;
use std::rc::Rc;

use quick_protobuf::Writer;
use sodiumoxide::crypto::generichash;

use crate::crypto_stream::{crypto_stream_xor_instance, Xor};
use std::borrow::Cow;
use std::cell::RefCell;

mod crypto_stream;
mod feed;
mod wire_format;

mod schema {
    #![allow(non_snake_case)]
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(unused_imports)]
    #![allow(clippy::all)]

    include!(concat!(env!("OUT_DIR"), "/protos/schema.rs"));
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Channel(u8);

impl Channel {
    const MAX_CHANNELS: usize = 128;
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message<'a> {
    Feed(schema::Feed<'a>),
    Handshake(schema::Handshake<'a>),
    Info(schema::Info),
    Have(schema::Have<'a>),
    Unhave(schema::Unhave),
    Want(schema::Want),
    Unwant(schema::Unwant),
    Request(schema::Request),
    Cancel(schema::Cancel),
    Data(schema::Data<'a>),
    Extension(&'a [u8]),
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Key([u8; 32]);

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct DiscoveryKey([u8; 32]);

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
struct Nonce([u8; 24]);

type Feed = feed::Feed<XXXStream>;

pub struct Protocol {
    destroyed: bool,
    encrypted: bool,
    key: Option<Key>,
    discovery_key: Option<DiscoveryKey>,
    feeds: HashMap<DiscoveryKey, Rc<RefCell<Feed>>>,
    max_feeds: usize,

    _local_feeds: Vec<Rc<RefCell<Feed>>>,

    _nonce: Option<Nonce>,
    _remote_nonce: Option<Nonce>,
    _xor: Option<Xor>,
    _remote_xor: Option<Xor>,
    _needs_key: bool,
    _keep_alive: u8,
}

pub struct ProtocolOpts {
    encrypted: Option<bool>,
}

pub struct FeedOptions {
    discovery_key: Option<DiscoveryKey>,
}

impl Protocol {
    pub fn new(opts: &ProtocolOpts) -> Protocol {
        Protocol {
            destroyed: false,
            encrypted: opts.encrypted.unwrap_or(true),
            key: None,
            discovery_key: None,
            feeds: HashMap::new(),
            max_feeds: 256,

            _local_feeds: Vec::new(),

            _nonce: None,
            _remote_nonce: None,
            _xor: None,
            _remote_xor: None,
            _needs_key: false,
            _keep_alive: 0,
        }
    }

    pub fn has(&self, key: &Key) -> bool {
        self.feeds.contains_key(&discovery_key(&key.0))
    }

    pub fn feed(&mut self, key: &Key, opts: FeedOptions) -> Option<Rc<RefCell<Feed>>> {
        if self.destroyed {
            return None;
        }

        let dk = opts.discovery_key.unwrap_or_else(|| discovery_key(&key.0));
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

        //        self.feeds.push(ch);

        let first = self.key.is_none();
        let mut feed: schema::Feed = schema::Feed {
            discoveryKey: Cow::Borrowed(&dk.0[..]),
            nonce: None,
        };

        if first {
            self.key = Some(key.clone());
            self.discovery_key = Some(dk.clone());

            if !self._same_key() {
                return None;
            }

            if self.encrypted {
                let mut nonce = [0; 24];
                random_bytes_into(&mut nonce);
                self._nonce = Some(Nonce(nonce));
                feed.nonce = self._nonce.as_ref().map(|n| Cow::from(Vec::from(&n.0[..])));

                self._xor = Some(crypto_stream_xor_instance(
                    &self._nonce.as_ref().unwrap().0,
                    &self.key.as_ref().unwrap().0,
                ));
                if let Some(ref remote_nonce) = self._remote_nonce {
                    self._remote_xor = Some(crypto_stream_xor_instance(
                        &remote_nonce.0,
                        &self.key.as_ref().unwrap().0,
                    ));
                }
            }

            if self._needs_key {
                self._needs_key = false;
                self._resume();
            }
        }

        let mut r#box = encode_feed(feed.clone(), ch.borrow().id.unwrap());
        if feed.nonce.is_none() && self.encrypted {
            self._xor
                .as_mut()
                .unwrap()
                .update(&r#box.clone(), &mut r#box);
        }
        self._keep_alive = 0;
        self.push(&r#box);

        if self.destroyed {
            return None;
        }

        if first {
            //            ch.handshake({
            //                id: this.id,
            //                live: this.live,
            //                userData: this.userData,
            //                extensions: this.extensions,
            //                ack: this.ack
            //            });
        }

        if ch.borrow()._buffer.as_ref().unwrap().len() > 0 {
            ch.borrow_mut()._resume();
        } else {
            ch.borrow_mut()._buffer = None
        }

        return Some(ch.clone());
    }

    fn push(&self, bytes: &[u8]) {
        unimplemented!()
    }

    fn _resume(&self) -> ! {
        unimplemented!()
    }

    fn _feed(&mut self, dk: &DiscoveryKey) -> Rc<RefCell<Feed>> {
        if let Some(ch) = self.feeds.get_mut(dk) {
            return ch.clone();
        }
        let ch = Feed::new(XXXStream);
        self.feeds.insert(dk.clone(), Rc::new(RefCell::new(ch)));
        self.feeds.get_mut(dk).unwrap().clone()
    }

    fn _same_key(&self) -> bool {
        unimplemented!()
    }

    fn _too_many_feeds(&self) -> ! {
        unimplemented!()
    }
}

// TODO rename
pub struct XXXStream;

impl feed::Stream for XXXStream {
    fn push(&mut self, bytes: &[u8]) {
        unimplemented!()
    }

    fn _push(&mut self, bytes: &[u8]) {
        unimplemented!()
    }
}

fn encode_feed(feed: schema::Feed, channel: Channel) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut w = Writer::new(&mut bytes);
    wire_format::write_msg(channel, &Message::Feed(feed), &mut w).unwrap();
    bytes
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
    // TODO init sodiumoxide somewhere else
    sodiumoxide::init().unwrap();
    sodiumoxide::randombytes::randombytes(n)
}

fn random_bytes_into(buf: &mut [u8]) {
    // TODO init sodiumoxide somewhere else
    sodiumoxide::init().unwrap();
    sodiumoxide::randombytes::randombytes_into(buf);
}

#[cfg(test)]
mod tests {
    use data_encoding::HEXUPPER;
    use std::borrow::Cow;

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
        let feed = schema::Feed {
            discoveryKey: Cow::Borrowed(&b"01234567890123456789012345678901"[..]),
            nonce: None,
        };
        assert_eq!(
            HEXUPPER.encode(&encode_feed(feed, Channel(42))),
            "24A0050A203031323334353637383930313233343536373839303132333435363738393031"
        )
    }
}
