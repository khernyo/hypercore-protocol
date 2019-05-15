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
    Handshake(schema::Handshake<'a>),
    Info(schema::Info),
    Have(schema::Have<'a>),
    Unhave(schema::Unhave),
    Want(schema::Want),
    Unwant(schema::Unwant),
    Request(schema::Request),
    Cancel(schema::Cancel),
    Data(schema::Data<'a>),
}
