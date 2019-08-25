use std::io::{BufReader, Read, Write};

use integer_encoding::{VarInt, VarIntReader, VarIntWriter};
use protobuf::{self, parse_from_reader, Message as _, ProtobufResult};

use crate::protocol::{Channel, Header, Message, MessageType};

pub(crate) fn write_msg(channel: Channel, msg: &Message) -> ProtobufResult<Vec<u8>> {
    log::trace!("write_msg({:?}, {:?})", channel, msg);
    let mut buf = Vec::new();
    write_msg_to_writer(channel, msg, &mut buf)?;
    log::trace!("write_msg => {:?}", buf);
    Ok(buf)
}

pub(crate) fn write_msg_to_writer<W: Write>(
    channel: Channel,
    msg: &Message,
    mut writer: W,
) -> ProtobufResult<()> {
    log::trace!("write_msg_to_writer({:?}, {:?})", channel, msg);
    let message_type = MessageType::from_message(&msg);
    let header = encode_header(Header {
        channel,
        message_type,
    });
    log::trace!(
        "write_msg_to_writer channel: {:?}, message_type: {:?} => header: {}",
        channel,
        message_type,
        header
    );

    let len = VarInt::required_space(u64::from(header)) + get_size(msg);
    log::trace!("write_msg_to_writer len: {}", len);

    writer.write_varint(len)?;
    writer.write_varint(header)?;
    write_message(msg, writer)?;
    Ok(())
}

fn read_msg(bytes: &[u8]) -> ProtobufResult<(Channel, Message)> {
    log::trace!("read_msg({:?})", bytes);
    let mut reader = BufReader::new(bytes);
    let (channel, msg) = read_msg_from_reader(&mut reader)?;
    log::trace!("read_msg channel: {:?}, message: {:?}", channel, msg);
    let mut remaining = Vec::new();
    assert_eq!(reader.read_to_end(&mut remaining)?, 0);
    Ok((channel, msg))
}

fn read_msg_from_reader<R: Read>(mut reader: R) -> ProtobufResult<(Channel, Message)> {
    log::trace!("read_msg_from_reader()");
    let len: usize = reader.read_varint()?;
    let header = reader.read_varint()?;
    log::trace!("read_msg_from_reader len: {}, header: {:?}", len, header);

    let header_len = VarInt::required_space(header);
    let msg_len = len - header_len;
    log::trace!(
        "read_msg_from_reader header_len: {}, msg_len: {}",
        header_len,
        msg_len
    );

    let Header {
        channel,
        message_type,
    } = decode_header(header);
    log::trace!(
        "read_msg_from_reader channel: {:?}, message_type: {:?}",
        channel,
        message_type
    );

    let msg = read_msg2(message_type, reader)?;

    Ok((channel, msg))
}

pub(crate) fn read_msg2<R: Read>(
    message_type: MessageType,
    mut reader: R,
) -> ProtobufResult<Message> {
    let msg = match message_type {
        MessageType::Feed => Message::Feed(parse_from_reader(&mut reader)?),
        MessageType::Handshake => Message::Handshake(parse_from_reader(&mut reader)?),
        MessageType::Info => Message::Info(parse_from_reader(&mut reader)?),
        MessageType::Have => Message::Have(parse_from_reader(&mut reader)?),
        MessageType::Unhave => Message::Unhave(parse_from_reader(&mut reader)?),
        MessageType::Want => Message::Want(parse_from_reader(&mut reader)?),
        MessageType::Unwant => Message::Unwant(parse_from_reader(&mut reader)?),
        MessageType::Request => Message::Request(parse_from_reader(&mut reader)?),
        MessageType::Cancel => Message::Cancel(parse_from_reader(&mut reader)?),
        MessageType::Data => Message::Data(parse_from_reader(&mut reader)?),
        MessageType::Extension => unimplemented!(),
    };
    Ok(msg)
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
            _ => panic!("Unknown message type: {}", value),
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

fn write_message<W: Write>(msg: &Message, mut writer: W) -> ProtobufResult<()> {
    match msg {
        Message::Feed(m) => m.write_to_writer(&mut writer),
        Message::Handshake(m) => m.write_to_writer(&mut writer),
        Message::Info(m) => m.write_to_writer(&mut writer),
        Message::Have(m) => m.write_to_writer(&mut writer),
        Message::Unhave(m) => m.write_to_writer(&mut writer),
        Message::Want(m) => m.write_to_writer(&mut writer),
        Message::Unwant(m) => m.write_to_writer(&mut writer),
        Message::Request(m) => m.write_to_writer(&mut writer),
        Message::Cancel(m) => m.write_to_writer(&mut writer),
        Message::Data(m) => m.write_to_writer(&mut writer),
        Message::Extension(bytes) => unimplemented!(),
    }
}

fn get_size(msg: &Message) -> usize {
    fn compute_size<M: protobuf::Message>(m: &M) -> usize {
        m.compute_size() as usize
    }
    match msg {
        Message::Feed(m) => compute_size(m),
        Message::Handshake(m) => compute_size(m),
        Message::Info(m) => compute_size(m),
        Message::Have(m) => compute_size(m),
        Message::Unhave(m) => compute_size(m),
        Message::Want(m) => compute_size(m),
        Message::Unwant(m) => compute_size(m),
        Message::Request(m) => compute_size(m),
        Message::Cancel(m) => compute_size(m),
        Message::Data(m) => compute_size(m),
        Message::Extension(bytes) => bytes.len(),
    }
}

fn channel_from(value: u16) -> Channel {
    assert!(
        (value as usize) < Channel::MAX_CHANNELS,
        "{} {}",
        value,
        value as usize
    );
    Channel(value as u8)
}

fn encode_header(header: Header) -> u16 {
    assert!((header.channel.0 as usize) < Channel::MAX_CHANNELS);
    u16::from(header.channel.0) << 4 | header.message_type as u16
}

pub(crate) fn decode_header(header: u16) -> Header {
    let message_type = MessageType::from(header as u8 & 0x0f);
    let channel = channel_from(header >> 4);
    Header {
        channel,
        message_type,
    }
}

#[cfg(test)]
mod tests {
    use crate::schema;

    use super::*;

    #[test]
    fn test_write_info() {
        let mut info = schema::Info::new();
        info.set_uploading(false);
        info.set_downloading(true);
        let msg = Message::Info(info);
        let v = write_msg(Channel(42), &msg).unwrap();

        assert_eq!(v, &[0x06, 0xa2, 0x05, 0x08, 0x00, 0x10, 0x01]);
    }

    #[test]
    fn test_read_info() {
        let bytes = &[0x06, 0xa2, 0x05, 0x08, 0x00, 0x10, 0x01];
        let mut info = schema::Info::new();
        info.set_uploading(false);
        info.set_downloading(true);
        let expected = (Channel(42), Message::Info(info));

        let result = read_msg(bytes).unwrap();
        assert_eq!(result, expected);
    }
}
