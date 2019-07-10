use quick_protobuf::Writer;
use sodiumoxide::crypto::generichash;
use std::collections::HashMap;

mod crypto_stream;
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

type DiscoveryKey = [u8; 32];

pub struct Protocol {
    feeds: HashMap<DiscoveryKey, ()>,
}

impl Protocol {
    pub fn new() -> Protocol {
        Protocol {
            feeds: HashMap::new(),
        }
    }

    pub fn has(&self, key: &[u8]) -> bool {
        self.feeds.contains_key(&discovery_key(key))
    }
}

fn encode_feed(feed: schema::Feed, channel: Channel) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut w = Writer::new(&mut bytes);
    wire_format::write_msg(channel, &Message::Feed(feed), &mut w);
    bytes
}

fn discovery_key(key: &[u8]) -> DiscoveryKey {
    let mut hasher = generichash::State::new(32, Some(key)).unwrap();
    hasher.update(b"hypercore").unwrap();
    let digest = hasher.finalize().unwrap();
    let mut result = [0u8; 32];
    result[..].clone_from_slice(digest.as_ref());
    result
}

fn random_bytes(n: usize) -> Vec<u8> {
    // TODO init sodiumoxide somewhere else
    sodiumoxide::init().unwrap();
    sodiumoxide::randombytes::randombytes(n)
}

#[cfg(test)]
mod tests {
    use data_encoding::HEXUPPER;
    use std::borrow::Cow;

    use super::*;

    #[test]
    fn test_discovery_key() {
        assert_eq!(
            HEXUPPER.encode(&discovery_key(b"01234567890123456789012345678901")),
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
