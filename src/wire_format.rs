use std::io::Write;

use quick_protobuf::sizeofs::*;
use quick_protobuf::writer::Writer;
use quick_protobuf::{BytesReader, MessageWrite};

use crate::{Channel, Message};

pub fn write_msg<W: Write>(
    channel: Channel,
    msg: &Message,
    writer: &mut Writer<W>,
) -> quick_protobuf::Result<()> {
    let msg_type = MessageType::from_message(&msg);
    let header = encode_header(channel, msg_type);
    let len = sizeof_varint(u64::from(header)) + get_size(msg);

    writer.write_varint(len as u64)?;
    writer.write_varint(u64::from(header))?;
    write_message(msg, writer)
}

pub fn read_msg<'a>(
    reader: &mut BytesReader,
    bytes: &'a [u8],
) -> quick_protobuf::Result<(Channel, Message<'a>)> {
    let len = reader.read_varint64(bytes)?;
    let header = reader.read_varint64(bytes)?;
    let header_len = sizeof_varint(header);
    let (channel, msg_type) = decode_header(header as u16);

    let msg_len = len as usize - header_len;
    let msg = match msg_type {
        MessageType::Feed => Message::Feed(reader.read_message_by_len(bytes, msg_len)?),
        MessageType::Handshake => Message::Handshake(reader.read_message_by_len(bytes, msg_len)?),
        MessageType::Info => Message::Info(reader.read_message_by_len(bytes, msg_len)?),
        MessageType::Have => Message::Have(reader.read_message_by_len(bytes, msg_len)?),
        MessageType::Unhave => Message::Unhave(reader.read_message_by_len(bytes, msg_len)?),
        MessageType::Want => Message::Want(reader.read_message_by_len(bytes, msg_len)?),
        MessageType::Unwant => Message::Unwant(reader.read_message_by_len(bytes, msg_len)?),
        MessageType::Request => Message::Request(reader.read_message_by_len(bytes, msg_len)?),
        MessageType::Cancel => Message::Cancel(reader.read_message_by_len(bytes, msg_len)?),
        MessageType::Data => Message::Data(reader.read_message_by_len(bytes, msg_len)?),
        MessageType::Extension => unimplemented!(),
    };

    Ok((channel, msg))
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum MessageType {
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

impl MessageType {
    fn from(value: u8) -> MessageType {
        use MessageType::*;
        match value {
            0 => Feed,
            1 => Handshake,
            2 => Info,
            3 => Have,
            4 => Unhave,
            5 => Want,
            6 => Unwant,
            7 => Request,
            8 => Cancel,
            9 => Data,
            15 => Extension,
            _ => panic!(),
        }
    }

    fn from_message(msg: &Message) -> MessageType {
        use MessageType::*;
        match msg {
            Message::Feed(_) => Feed,
            Message::Handshake(_) => Handshake,
            Message::Info(_) => Info,
            Message::Have(_) => Have,
            Message::Unhave(_) => Unhave,
            Message::Want(_) => Want,
            Message::Unwant(_) => Unwant,
            Message::Request(_) => Request,
            Message::Cancel(_) => Cancel,
            Message::Data(_) => Data,
            Message::Extension(_) => Extension,
        }
    }
}

fn write_message<W: Write>(msg: &Message, w: &mut Writer<W>) -> quick_protobuf::Result<()> {
    match msg {
        Message::Feed(m) => m.write_message(w),
        Message::Handshake(m) => m.write_message(w),
        Message::Info(m) => m.write_message(w),
        Message::Have(m) => m.write_message(w),
        Message::Unhave(m) => m.write_message(w),
        Message::Want(m) => m.write_message(w),
        Message::Unwant(m) => m.write_message(w),
        Message::Request(m) => m.write_message(w),
        Message::Cancel(m) => m.write_message(w),
        Message::Data(m) => m.write_message(w),
        Message::Extension(bytes) => unimplemented!(),
    }
}

fn get_size(msg: &Message) -> usize {
    match msg {
        Message::Feed(m) => m.get_size(),
        Message::Handshake(m) => m.get_size(),
        Message::Info(m) => m.get_size(),
        Message::Have(m) => m.get_size(),
        Message::Unhave(m) => m.get_size(),
        Message::Want(m) => m.get_size(),
        Message::Unwant(m) => m.get_size(),
        Message::Request(m) => m.get_size(),
        Message::Cancel(m) => m.get_size(),
        Message::Data(m) => m.get_size(),
        Message::Extension(bytes) => bytes.len(),
    }
}

fn channel_from(value: u16) -> Channel {
    assert!((value as usize) < Channel::MAX_CHANNELS);
    Channel(value as u8)
}

fn encode_header(channel: Channel, msg_type: MessageType) -> u16 {
    assert!((channel.0 as usize) < Channel::MAX_CHANNELS);
    u16::from(channel.0) << 4 | msg_type as u16
}

fn decode_header(header: u16) -> (Channel, MessageType) {
    let msg_type = MessageType::from(header as u8 & 0x0f);
    let channel = channel_from(header >> 4);
    (channel, msg_type)
}

#[cfg(test)]
mod tests {
    use crate::schema;

    use super::*;

    #[test]
    fn test_write_info() {
        let msg = Message::Info(schema::Info {
            uploading: Some(false),
            downloading: Some(true),
        });
        let mut v = Vec::new();
        let mut w = Writer::new(&mut v);
        write_msg(Channel(42), &msg, &mut w).unwrap();

        assert_eq!(v, &[0x06, 0xa2, 0x05, 0x08, 0x00, 0x10, 0x01]);
    }

    #[test]
    fn test_read_info() {
        let bytes = &[0x06, 0xa2, 0x05, 0x08, 0x00, 0x10, 0x01];
        let expected = (
            Channel(42),
            Message::Info(schema::Info {
                uploading: Some(false),
                downloading: Some(true),
            }),
        );

        let mut r = BytesReader::from_bytes(bytes);
        let result = read_msg(&mut r, bytes).unwrap();
        assert_eq!(result, expected);
    }
}
