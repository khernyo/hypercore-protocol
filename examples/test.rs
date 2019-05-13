use std::io::Write;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use slog::{trace, Drain, Logger};

use hypercore_protocol::{
    FeedEvent, FeedEventEmitter, FeedOptions, Key, Protocol, ProtocolOpts, Stream,
};

struct XStream<W: Write> {
    logger: Logger,
    output: W,
}
impl<W: Write> Stream for XStream<W> {
    fn _push(&mut self, bytes: &mut [u8]) {
        trace!(self.logger, "XStream::_push({:?})", bytes);
        self.output.write_all(bytes).unwrap();
        trace!(self.logger, "XStream::_push done");
    }
}

#[derive(Clone)]
struct ChannelStream {
    logger: Logger,
    sender: Sender<Event>,
}
impl Stream for ChannelStream {
    fn _push(&mut self, bytes: &mut [u8]) {
        trace!(self.logger, "ChannelStream::_push({:?})", bytes);
        self.sender.send(Event::Send(bytes.to_owned())).unwrap();
        trace!(self.logger, "ChannelStream::_push done");
    }
}

struct Emitter<'a>(&'a mut Vec<FeedEvent>);
impl FeedEventEmitter for Emitter<'_> {
    fn emit(&mut self, event: FeedEvent) {
        self.0.push(event);
    }
}

const KEY: Key = Key(*b"01234567890123456789012345678901");

fn main() {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = std::sync::Mutex::new(drain).fuse();
    let logger = slog::Logger::root(
        drain,
        slog::o!(
            "thread" => slog::FnValue(|_| {
                format!("{:?}:{:?}", thread::current().id(), thread::current().name())
            }),
            "place" => slog::FnValue(move |info| {
                format!("{}:{} {}", info.file(), info.line(), info.module())
            }),
        ),
    );
    let _scope_guard = slog_scope::set_global_logger(logger.clone());
    let _log_guard = slog_stdlog::init().unwrap();

    let (ch1to2_sender, ch1to2_receiver) = std::sync::mpsc::channel();
    let (ch2to1_sender, ch2to1_receiver) = std::sync::mpsc::channel();

    let logger1 = logger.clone();
    let handle1 = thread::Builder::new()
        .name("t1".to_owned())
        .spawn(|| {
            trace!(logger1, "Running thread1");
            run(logger1, &KEY, ch1to2_sender, ch2to1_receiver);
        })
        .unwrap();
    let logger2 = logger.clone();
    let handle2 = thread::Builder::new()
        .name("t2".to_owned())
        .spawn(|| {
            trace!(logger2, "Running thread2");
            run(logger2, &KEY, ch2to1_sender, ch1to2_receiver);
        })
        .unwrap();

    handle1.join().unwrap();
    handle2.join().unwrap();
}

fn run(logger: Logger, key: &Key, sender: Sender<Vec<u8>>, receiver: Receiver<Vec<u8>>) {
    let (s, r) = std::sync::mpsc::channel();

    let protocol_opts = ProtocolOpts {
        id: None,
        live: None,
        user_data: None,
        ack: None,
        encrypted: None,
        extensions: None,
    };
    let stream = ChannelStream {
        logger: logger.clone(),
        sender: s.clone(),
    };
    let mut feed_events = Vec::new();
    let emitter = Emitter(&mut feed_events);

    let protocol = Protocol::new(logger.clone(), emitter, stream, &protocol_opts);

    let feed_opts = FeedOptions {
        discovery_key: None,
    };

    let logger1 = logger.clone();
    let name = format!("{}r", thread::current().name().unwrap());
    let bytes_reader_handle = thread::Builder::new()
        .name(name)
        .spawn(move || {
            for bytes in receiver {
                trace!(logger1, "Received bytes: {:?}", bytes);
                s.send(Event::Receive(bytes)).unwrap();
            }
        })
        .unwrap();

    event_loop(logger, key, feed_opts, r, protocol, sender);

    bytes_reader_handle.join().unwrap();
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Event {
    Receive(Vec<u8>),
    Send(Vec<u8>),
}

fn event_loop(
    logger: Logger,
    key: &Key,
    feed_opts: FeedOptions,
    receiver: Receiver<Event>,
    mut protocol: Protocol<Emitter, ChannelStream>,
    sender: Sender<Vec<u8>>,
) {
    protocol.feed(key, feed_opts);

    for mut event in receiver {
        trace!(logger, "event_loop: {:?}", event);
        match event {
            Event::Receive(ref mut bytes) => protocol._write(bytes),
            Event::Send(bytes) => sender.send(bytes).unwrap(),
        }
        trace!(logger, "event_loop next");
    }

    trace!(logger, "event_loop exit");
}
