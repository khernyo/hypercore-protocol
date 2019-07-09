use blake2_rfc::blake2b::blake2b;
use std::collections::HashMap;

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

fn discovery_key(key: &[u8]) -> DiscoveryKey {
    let mut result = [0u8; 32];
    let hash = blake2b(32, key, b"hypercore");
    result[..].clone_from_slice(hash.as_bytes());
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_key() {
        assert_eq!(
            data_encoding::HEXUPPER.encode(&discovery_key(b"01234567890123456789012345678901")),
            "103E9C9562455F70DFE3F3F9F1DC0CF8548D72D6C4B3C5AC1B44EAEFDB6F7E65"
        );
    }
}
