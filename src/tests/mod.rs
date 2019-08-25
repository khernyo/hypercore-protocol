mod protocol_pair;

use std::ops::Deref;
use std::thread;

use once_cell::sync::Lazy;
use slog::{Drain, Logger};
use slog_scope::GlobalLoggerGuard;

use crate::protocol::{FeedOptions, Id, Key, ProtocolOpts};
use crate::tests::protocol_pair::ProtocolPair;
use crate::FeedEvent;

const KEY: Key = Key(*b"01234567890123456789012345678901");
const OTHER_KEY: Key = Key(*b"12345678901234567890123456789012");

fn init() {
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
    init();

    let opts = ProtocolOpts::default();
    let mut pp = ProtocolPair::new(&opts, &opts);

    let feed_opts = FeedOptions {
        discovery_key: None,
    };

    pp.a.protocol.feed(&KEY, feed_opts.clone());
    pp.b.protocol.feed(&KEY, feed_opts.clone());

    pp.run();

    assert_eq!(
        pp.a.feed_events.borrow()[..],
        vec![FeedEvent::Handshake][..]
    );
    assert_eq!(
        pp.b.feed_events.borrow()[..],
        vec![FeedEvent::Handshake][..]
    );

    assert_eq!(pp.a.sent.borrow().len(), 2);
    assert_eq!(pp.b.sent.borrow().len(), 2);

    assert_ne!(pp.a.sent.borrow()[0], pp.b.sent.borrow()[0]);
    assert_ne!(pp.a.sent.borrow()[1], pp.b.sent.borrow()[1]);
}

#[test]
fn basic_with_early_messages() {
    // I'm not sure this test is necessary or valid even. It delivers messages sooner compared
    //  to `basic`

    init();

    let opts = ProtocolOpts::default();
    let mut pp = ProtocolPair::new(&opts, &opts);

    let feed_opts = FeedOptions {
        discovery_key: None,
    };

    pp.run();
    pp.a.protocol.feed(&KEY, feed_opts.clone());
    pp.run();
    pp.b.protocol.feed(&KEY, feed_opts.clone());
    pp.run();

    assert_eq!(
        pp.a.feed_events.borrow()[..],
        vec![FeedEvent::Handshake][..]
    );
    assert_eq!(pp.b.feed_events.borrow()[..], vec![][..]);

    assert_eq!(pp.a.sent.borrow().len(), 2);
    assert_eq!(pp.b.sent.borrow().len(), 2);

    assert_ne!(pp.a.sent.borrow()[0], pp.b.sent.borrow()[0]);
    assert_ne!(pp.a.sent.borrow()[1], pp.b.sent.borrow()[1]);
}

#[test]
fn basic_with_handshake_options() {
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
    let mut pp = ProtocolPair::new(&opts_a, &opts_b);

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
