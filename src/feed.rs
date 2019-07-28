use std::io::Write;

use quick_protobuf::writer::Writer;

use crate::schema;
use crate::wire_format::write_msg;
use crate::{Channel, DiscoveryKey, Key, Message};

pub trait Stream {
    fn push(&mut self, bytes: &[u8]);
    fn _push(&mut self, bytes: &[u8]);
}

pub struct Feed<S: Stream> {
    pub key: Option<Key>,
    pub discovery_key: Option<DiscoveryKey>,
    stream: S,

    pub id: Option<Channel>,
    remote_id: Option<()>,
    header: (),
    header_length: (),
    closed: bool,

    pub _buffer: Option<Vec<u8>>,
}

impl<S: Stream> Feed<S> {
    pub fn new(stream: S) -> Feed<S> {
        Feed {
            key: None,
            discovery_key: None,
            stream,
            id: None,
            remote_id: None,
            header: (),
            header_length: (),
            closed: false,
            _buffer: None,
        }
    }

    pub fn handshake(&mut self, handshake: schema::Handshake) {
        dbg!(&handshake);
        let mut bytes = Vec::new();
        let mut writer = Writer::new(&mut bytes);
        write_msg(
            self.id.unwrap(),
            &Message::Handshake(handshake),
            &mut writer,
        )
        .unwrap();
        self.stream._push(&bytes);
    }

    pub fn _resume(&mut self) {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use data_encoding::HEXLOWER;

    struct DummyStream<'a>(&'a mut Vec<Vec<u8>>);
    impl Stream for DummyStream<'_> {
        fn push(&mut self, bytes: &[u8]) {
            unimplemented!()
        }

        fn _push(&mut self, bytes: &[u8]) {
            self.0.push(bytes.to_owned());
        }
    }

    #[test]
    fn send_handshake() {
        let mut stream_bytes = Vec::new();
        let mut feed = Feed::new(DummyStream(&mut stream_bytes));
        feed.id = Some(Channel(0));
        feed.handshake(schema::Handshake {
            id: Some(b"foo"[..].into()),
            live: Some(true),
            userData: Some(b"bar"[..].into()),
            extensions: vec!["baz"[..].into()],
            ack: Some(true),
        });
        assert_eq!(
            stream_bytes.iter().map(|bytes| HEXLOWER.encode(&bytes)).collect::<Vec<_>>(),
            vec!["14010a03666f6f10011a03626172220362617a2801"]
        );
    }
}
