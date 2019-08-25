// TODO integer_encoding crate simply truncates when casting u64 to e.g. u16. It should
//  report an error instead.

mod crypto_stream;
mod feed;
pub mod protocol;
mod wire_format;

#[cfg(test)]
mod tests;

pub use feed::{FeedEvent, FeedEventEmitter};

include!(concat!(env!("OUT_DIR"), "/protos/mod.rs"));
