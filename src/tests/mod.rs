mod protocol_pair;

use std::ops::Deref;
use std::thread;

use once_cell::sync::Lazy;
use slog::{Drain, Logger};
use slog_scope::GlobalLoggerGuard;

use crate::protocol::{DiscoveryKey, FeedOptions, Id, Key, Message, ProtocolOpts};
use crate::schema;
use crate::tests::protocol_pair::ProtocolPair;
use crate::FeedEvent;

const KEY: Key = Key(*b"01234567890123456789012345678901");
const OTHER_KEY: Key = Key(*b"12345678901234567890123456789012");

fn init_logger() {
    static LOGGER: Lazy<Logger> = Lazy::new(|| {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let drain = std::sync::Mutex::new(drain).fuse();
        slog::Logger::root(
            drain,
            slog::o!(
                "thread" => slog::FnValue(|_| {
                    format!("{:?}:{:?}", thread::current().id(), thread::current().name())
                }),
                "place" => slog::FnValue(move |info| {
                    format!("{}:{} {}", info.file(), info.line(), info.module())
                }),
            ),
        )
    });
    let _ = LOGGER.deref();

    static SCOPE_GUARD: Lazy<GlobalLoggerGuard> =
        Lazy::new(|| slog_scope::set_global_logger(LOGGER.clone()));
    let _ = SCOPE_GUARD.deref();

    static LOG_GUARD: Lazy<()> = Lazy::new(|| slog_stdlog::init().unwrap());
    let _ = LOG_GUARD.deref();

    static ENV_LOGGER_GUARD: Lazy<()> = Lazy::new(|| {
        let _ = env_logger::builder().is_test(true).try_init();
    });
    let _ = ENV_LOGGER_GUARD.deref();
}

#[test]
fn basic() {
    init_logger();

    let opts = ProtocolOpts::default();
    let mut pp = ProtocolPair::new(None, &opts, &opts);

    let feed_opts = FeedOptions {
        discovery_key: None,
    };

    pp.a.protocol.feed(&KEY, feed_opts.clone());
    pp.b.protocol.feed(&KEY, feed_opts.clone());

    pp.run();

    let dk = DiscoveryKey([
        16, 62, 156, 149, 98, 69, 95, 112, 223, 227, 243, 249, 241, 220, 12, 248, 84, 141, 114,
        214, 196, 179, 197, 172, 27, 68, 234, 239, 219, 111, 126, 101,
    ]);
    assert_eq!(
        pp.a.feed_events.borrow()[..],
        vec![FeedEvent::Feed(dk.clone()), FeedEvent::Handshake][..]
    );
    assert_eq!(
        pp.b.feed_events.borrow()[..],
        vec![FeedEvent::Feed(dk), FeedEvent::Handshake][..]
    );

    assert_eq!(pp.a.sent.borrow().len(), 2);
    assert_eq!(pp.b.sent.borrow().len(), 2);

    assert_ne!(pp.a.sent.borrow()[0], pp.b.sent.borrow()[0]);
    assert_ne!(pp.a.sent.borrow()[1], pp.b.sent.borrow()[1]);
}

#[test]
#[ignore]
fn basic_with_early_messages() {
    // I'm not sure this test is necessary or valid even. It delivers messages sooner compared
    //  to `basic`

    init_logger();

    let opts = ProtocolOpts::default();
    let mut pp = ProtocolPair::new(None, &opts, &opts);

    let feed_opts = FeedOptions {
        discovery_key: None,
    };

    pp.run();
    pp.a.protocol.feed(&KEY, feed_opts.clone());
    pp.run();
    pp.b.protocol.feed(&KEY, feed_opts.clone());
    pp.run();

    let dk = DiscoveryKey([
        16, 62, 156, 149, 98, 69, 95, 112, 223, 227, 243, 249, 241, 220, 12, 248, 84, 141, 114,
        214, 196, 179, 197, 172, 27, 68, 234, 239, 219, 111, 126, 101,
    ]);
    assert_eq!(
        pp.a.feed_events.borrow()[..],
        vec![FeedEvent::Feed(dk.clone()), FeedEvent::Handshake][..]
    );
    assert_eq!(pp.b.feed_events.borrow()[..], vec![FeedEvent::Feed(dk)][..]);

    assert_eq!(pp.a.sent.borrow().len(), 2);
    assert_eq!(pp.b.sent.borrow().len(), 2);

    assert_ne!(pp.a.sent.borrow()[0], pp.b.sent.borrow()[0]);
    assert_ne!(pp.a.sent.borrow()[1], pp.b.sent.borrow()[1]);
}

#[test]
fn basic_with_handshake_options() {
    init_logger();

    let data = vec![
        "eeaa62fbb11ba521cce58cf3fae42deb15d94a0436fc7fa0cbba8f130e7c0499".as_ref(),
        "8c797667bf307d82c51a8308fe477b781a13708e0ec1f2cc7f497392574e2464".as_ref(),
    ]
    .join("")
    .as_bytes()
    .to_vec();

    let opts_a = ProtocolOpts {
        id: Some(Id([b'a'; 32])),
        live: Some(true),
        user_data: Some(data.clone()),
        ..Default::default()
    };
    let opts_b = ProtocolOpts {
        id: Some(Id([b'b'; 32])),
        live: Some(false),
        ack: Some(true),
        ..Default::default()
    };
    let mut pp = ProtocolPair::new(None, &opts_a, &opts_b);

    pp.a.protocol.feed(&KEY, FeedOptions::default());
    pp.b.protocol.feed(&KEY, FeedOptions::default());

    pp.run();

    assert_eq!(pp.a.protocol.id, Id([b'a'; 32]));
    assert_eq!(pp.a.protocol.live, true);
    assert_eq!(pp.a.protocol.ack, false);
    assert_eq!(pp.a.protocol.user_data, Some(data.clone()));
    assert_eq!(*pp.a.protocol.remote_id.borrow(), Some(Id([b'b'; 32])));
    assert_eq!(pp.a.protocol.remote_live.get(), Some(false));
    assert_eq!(*pp.a.protocol.remote_user_data.borrow(), None);
    assert_eq!(pp.a.protocol.remote_ack.get(), Some(true));

    assert_eq!(pp.b.protocol.id, Id([b'b'; 32]));
    assert_eq!(pp.b.protocol.live, false);
    assert_eq!(pp.b.protocol.ack, true);
    assert_eq!(pp.b.protocol.user_data, None);
    assert_eq!(*pp.b.protocol.remote_id.borrow(), Some(Id([b'a'; 32])));
    assert_eq!(pp.b.protocol.remote_live.get(), Some(true));
    assert_eq!(*pp.b.protocol.remote_user_data.borrow(), Some(data));
    assert_eq!(pp.b.protocol.remote_ack.get(), Some(false));
}

#[test]
fn send_messages() {
    init_logger();

    let mut pp = ProtocolPair::new(None, &ProtocolOpts::default(), &ProtocolOpts::default());

    let ch1 = pp.a.protocol.feed(&KEY, FeedOptions::default());
    let ch2 = pp.b.protocol.feed(&KEY, FeedOptions::default());

    pp.run();

    fn single<T: Sized, I: Iterator<Item = T>>(mut iter: I) -> T {
        let v = iter.next().unwrap();
        assert!(iter.next().is_none());
        v
    }

    let a_feed_event = single(pp.a.feed_events.borrow().iter().filter_map(|e| e.as_feed())).clone();
    assert_eq!(
        &a_feed_event,
        ch2.unwrap().borrow().discovery_key.as_ref().unwrap()
    );
    let b_feed_event = single(pp.b.feed_events.borrow().iter().filter_map(|e| e.as_feed())).clone();
    assert_eq!(
        &b_feed_event,
        ch1.unwrap().borrow().discovery_key.as_ref().unwrap()
    );

    return; // TODO

    //    ch2.on('data', function(data) {
    //        t.same(data, { index: 42, signature: null, value: bufferFrom('hi'), nodes: [] })
    //    })
    unimplemented!();

    //    ch1.data({ index: 42, value: bufferFrom('hi') })
    let mut data = schema::Data::new();
    data.set_index(42);
    data.set_value(b"hi".to_vec());
    ch1.unwrap().get_mut().data(data);

    //    ch2.on('request', function(request) {
    //        t.same(request, { index: 10, hash: false, bytes: 0, nodes: 0 })
    //    })
    unimplemented!();

    //    ch1.request({ index: 10 })
    //
    //    ch2.on('cancel', function(cancel) {
    //        t.same(cancel, { index: 100, hash: false, bytes: 0 })
    //    })
    unimplemented!();

    //    ch1.cancel({ index: 100 })
    //
    //    ch1.on('want', function(want) {
    //        t.same(want, { start: 10, length: 100 })
    //    })
    unimplemented!();

    //    ch2.want({ start: 10, length: 100 })
    //
    //    ch1.on('info', function(info) {
    //        t.same(info, { uploading: false, downloading: true })
    //    })
    unimplemented!();

    //    ch2.info({ uploading: false, downloading: true })
    //
    //    ch1.on('unwant', function(unwant) {
    //        t.same(unwant, { start: 11, length: 100 })
    //    })
    unimplemented!();

    //    ch2.unwant({ start: 11, length: 100 })
    //
    //    ch1.on('unhave', function(unhave) {
    //        t.same(unhave, { start: 18, length: 100 })
    //    })
    unimplemented!();

    //    ch2.unhave({ start: 18, length: 100 })
    //
    //    ch1.on('have', function(have) {
    //        t.same(have, { start: 10, length: 10, bitfield: null })
    //    })
    unimplemented!();

    //    ch2.have({ start: 10, length: 10 })
    //
    //    a.pipe(b).pipe(a)
    unimplemented!();
}
