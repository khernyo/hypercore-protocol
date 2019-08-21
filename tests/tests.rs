extern crate hypercore_protocol;

include!("protocol_pair.rs");

use std::ops::Deref;
use std::thread;

use once_cell::sync::Lazy;
use slog::{Drain, Logger};
use slog_scope::GlobalLoggerGuard;

use hypercore_protocol::{FeedOptions, Key};

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

    let mut pp = ProtocolPair::new();

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

    let mut pp = ProtocolPair::new();

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
