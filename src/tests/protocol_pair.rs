use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use log::trace;

use crate::protocol::{Protocol, ProtocolOpts, Stream};
use crate::{FeedEvent, FeedEventEmitter};

pub struct ProtocolPair {
    pub a: ProtocolX,
    pub b: ProtocolX,
}

impl ProtocolPair {
    pub fn new() -> Self {
        let (sender1, receiver1) = mpsc::channel();
        let (sender2, receiver2) = mpsc::channel();

        let opts = ProtocolOpts::default();
        Self {
            a: ProtocolX::new(&opts, sender1, receiver2),
            b: ProtocolX::new(&opts, sender2, receiver1),
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
    pub protocol: Protocol<Emitter, ChannelStream>,
    receiver: mpsc::Receiver<Vec<u8>>,

    pub sent: Rc<RefCell<Vec<Vec<u8>>>>,
    pub feed_events: Rc<RefCell<Vec<FeedEvent>>>,
}

impl ProtocolX {
    fn new(
        protocol_opts: &ProtocolOpts,
        sender: mpsc::Sender<Vec<u8>>,
        receiver: mpsc::Receiver<Vec<u8>>,
    ) -> Self {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let feed_events = Rc::new(RefCell::new(Vec::new()));
        Self {
            protocol: Protocol::new(
                None,
                Emitter(feed_events.clone()),
                ChannelStream {
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
                    trace!("Received bytes: {:?}", bytes);
                    self.protocol._write(&mut bytes);
                }
                Err(_) => break,
            }
        }
        got_message
    }
}

pub struct ChannelStream {
    sender: mpsc::Sender<Vec<u8>>,
    sent: Rc<RefCell<Vec<Vec<u8>>>>,
}

impl Stream for ChannelStream {
    fn _push(&mut self, bytes: &mut [u8]) {
        trace!("Sending bytes: {:?}", bytes);
        self.sender.send(bytes.to_vec()).unwrap();
        self.sent.borrow_mut().push(bytes.to_vec());
    }
}

pub struct Emitter(Rc<RefCell<Vec<FeedEvent>>>);
impl FeedEventEmitter for Emitter {
    fn emit(&mut self, event: FeedEvent) {
        trace!(
            "Emitting from {:x}: {:?}",
            self as *const Emitter as usize,
            event
        );
        self.0.borrow_mut().push(event);
    }
}
