use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use slog::{o, trace, Drain, Logger};

use crate::protocol::{DiscoveryKey, Protocol, ProtocolOpts, Stream};
use crate::{FeedEvent, FeedEventEmitter};

pub struct ProtocolPair {
    pub a: ProtocolX,
    pub b: ProtocolX,
}

impl ProtocolPair {
    pub fn new<L: Into<Option<slog::Logger>>>(
        logger: L,
        opts_a: &ProtocolOpts,
        opts_b: &ProtocolOpts,
    ) -> Self {
        let (sender1, receiver1) = mpsc::channel();
        let (sender2, receiver2) = mpsc::channel();

        let logger = logger
            .into()
            .unwrap_or_else(|| Logger::root(slog_stdlog::StdLog.fuse(), o!()));
        let a_logger = logger.new(o!("endpoint" => "a"));
        let b_logger = logger.new(o!("endpoint" => "b"));

        Self {
            a: ProtocolX::new(a_logger, opts_a, sender1, receiver2),
            b: ProtocolX::new(b_logger, opts_b, sender2, receiver1),
        }
    }

    pub fn run(&mut self) {
        loop {
            let a_got_message = self.a.process();
            let b_got_message = self.b.process();
            if !a_got_message && !b_got_message {
                break;
            }
        }
    }
}

pub struct ProtocolX {
    logger: Logger,
    pub protocol: Protocol<Emitter, ChannelStream>,
    receiver: mpsc::Receiver<Vec<u8>>,

    pub sent: Rc<RefCell<Vec<Vec<u8>>>>,
    pub feed_events: Rc<RefCell<Vec<(DiscoveryKey, FeedEvent)>>>,
}

impl ProtocolX {
    fn new(
        logger: Logger,
        protocol_opts: &ProtocolOpts,
        sender: mpsc::Sender<Vec<u8>>,
        receiver: mpsc::Receiver<Vec<u8>>,
    ) -> Self {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let feed_events = Rc::new(RefCell::new(Vec::new()));
        Self {
            logger: logger.clone(),
            protocol: Protocol::new(
                logger.clone(),
                Emitter {
                    logger: logger.clone(),
                    feed_events: feed_events.clone(),
                },
                ChannelStream {
                    logger,
                    sender,
                    sent: sent.clone(),
                },
                &protocol_opts,
            ),
            receiver,
            sent,
            feed_events,
        }
    }

    fn process(&mut self) -> bool {
        let mut got_message = false;
        loop {
            match self.receiver.try_recv() {
                Ok(mut bytes) => {
                    got_message = true;
                    trace!(self.logger, "Received bytes: {:?}", bytes);
                    self.protocol._write(&mut bytes);
                }
                Err(_) => break,
            }
        }
        got_message
    }
}

pub struct ChannelStream {
    logger: Logger,
    sender: mpsc::Sender<Vec<u8>>,
    sent: Rc<RefCell<Vec<Vec<u8>>>>,
}

impl Stream for ChannelStream {
    fn _push(&mut self, bytes: &mut [u8]) {
        trace!(self.logger, "Sending bytes: {:?}", bytes);
        self.sender.send(bytes.to_vec()).unwrap();
        self.sent.borrow_mut().push(bytes.to_vec());
    }
}

pub struct Emitter {
    logger: Logger,
    feed_events: Rc<RefCell<Vec<(DiscoveryKey, FeedEvent)>>>,
}
impl FeedEventEmitter for Emitter {
    fn emit(&mut self, discovery_key: &DiscoveryKey, event: FeedEvent) {
        trace!(
            self.logger,
            "Emitting from {:?}: {:?}",
            discovery_key,
            event
        );
        self.feed_events
            .borrow_mut()
            .push((discovery_key.clone(), event));
    }
}
