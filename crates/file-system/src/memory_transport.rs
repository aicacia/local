use alloc::{sync::Arc, vec::Vec};
use core::{
    convert::Infallible,
    future::{Future, ready},
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
    task::{Context, Poll},
};
use std::sync::Mutex;

use futures_core::Stream;
use tokio::sync::mpsc;

use crate::Transport;

type Message<P> = (P, Vec<u8>);
type Sender<P> = mpsc::UnboundedSender<Message<P>>;
pub type MemoryTransportMutator = Arc<dyn Fn(&mut Vec<u8>) + Send + Sync>;

pub struct MemoryIncoming<P>(mpsc::UnboundedReceiver<Message<P>>);

impl<P> Stream for MemoryIncoming<P> {
    type Item = Message<P>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(context)
    }
}

struct Endpoint<P> {
    online: AtomicBool,
    sender: Sender<P>,
    pending: Mutex<Vec<Message<P>>>,
}

impl<P> Endpoint<P> {
    fn receive(&self, message: Message<P>) {
        if self.online.load(Ordering::Acquire) {
            let _ = self.sender.send(message);
        } else {
            self.pending
                .lock()
                .expect("memory transport lock poisoned")
                .push(message);
        }
    }

    fn set_online(&self, online: bool) {
        self.online.store(online, Ordering::Release);
        if !online {
            return;
        }
        let pending =
            core::mem::take(&mut *self.pending.lock().expect("memory transport lock poisoned"));
        for message in pending {
            let _ = self.sender.send(message);
        }
    }
}

struct MemoryTransportState<P> {
    endpoint: Arc<Endpoint<P>>,
    peers: Vec<(P, Arc<Endpoint<P>>)>,
    incoming: Mutex<Option<MemoryIncoming<P>>>,
    outbound: Mutex<Vec<(P, Vec<u8>)>>,
    mutator: Option<MemoryTransportMutator>,
}

pub struct MemoryTransport<P> {
    peer: P,
    state: Arc<MemoryTransportState<P>>,
}

impl<P> Clone for MemoryTransport<P>
where
    P: Clone,
{
    fn clone(&self) -> Self {
        Self {
            peer: self.peer.clone(),
            state: Arc::clone(&self.state),
        }
    }
}

impl<P> MemoryTransport<P>
where
    P: Clone + Eq + Send + Sync + 'static,
{
    #[must_use]
    pub fn pair(left_peer: P, right_peer: P) -> (Self, Self) {
        Self::pair_with_mutators(left_peer, right_peer, None, None)
    }

    #[must_use]
    pub fn pair_with_mutators(
        left_peer: P,
        right_peer: P,
        left_mutator: Option<MemoryTransportMutator>,
        right_mutator: Option<MemoryTransportMutator>,
    ) -> (Self, Self) {
        let (left_sender, left_receiver) = mpsc::unbounded_channel();
        let (right_sender, right_receiver) = mpsc::unbounded_channel();
        let left_endpoint = Arc::new(Endpoint {
            online: AtomicBool::new(true),
            sender: left_sender,
            pending: Mutex::new(Vec::new()),
        });
        let right_endpoint = Arc::new(Endpoint {
            online: AtomicBool::new(true),
            sender: right_sender,
            pending: Mutex::new(Vec::new()),
        });
        let left = Self {
            peer: left_peer.clone(),
            state: Arc::new(MemoryTransportState {
                endpoint: Arc::clone(&left_endpoint),
                peers: vec![(right_peer.clone(), Arc::clone(&right_endpoint))],
                incoming: Mutex::new(Some(MemoryIncoming(left_receiver))),
                outbound: Mutex::new(Vec::new()),
                mutator: left_mutator,
            }),
        };
        let right = Self {
            peer: right_peer.clone(),
            state: Arc::new(MemoryTransportState {
                endpoint: right_endpoint,
                peers: vec![(left_peer, left_endpoint)],
                incoming: Mutex::new(Some(MemoryIncoming(right_receiver))),
                outbound: Mutex::new(Vec::new()),
                mutator: right_mutator,
            }),
        };
        (left, right)
    }

    pub fn set_online(&self, online: bool) {
        self.state.endpoint.set_online(online);
        if !online {
            return;
        }
        let outbound = core::mem::take(
            &mut *self
                .state
                .outbound
                .lock()
                .expect("memory transport lock poisoned"),
        );
        for (peer, data) in outbound {
            self.deliver(peer, data);
        }
    }

    #[must_use]
    pub fn is_online(&self) -> bool {
        self.state.endpoint.online.load(Ordering::Acquire)
    }

    fn deliver(&self, peer: P, mut data: Vec<u8>) {
        if !self.is_online() {
            self.state
                .outbound
                .lock()
                .expect("memory transport lock poisoned")
                .push((peer, data));
            return;
        }
        if let Some(mutator) = &self.state.mutator {
            mutator(&mut data);
        }
        let endpoint = self
            .state
            .peers
            .iter()
            .find_map(|(candidate, endpoint)| (*candidate == peer).then_some(endpoint))
            .expect("unknown memory transport peer");
        endpoint.receive((self.peer.clone(), data));
    }
}

impl<P> Transport for MemoryTransport<P>
where
    P: Clone + Eq + Send + Sync + 'static,
{
    type Error = Infallible;
    type PeerId = P;
    type Incoming = MemoryIncoming<P>;

    fn send(
        &self,
        peer: Self::PeerId,
        data: Vec<u8>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.deliver(peer, data);
        ready(Ok(()))
    }

    fn broadcast(&self, data: Vec<u8>) -> impl Future<Output = Result<(), Self::Error>> + Send {
        for (peer, _) in &self.state.peers {
            self.deliver(peer.clone(), data.clone());
        }
        ready(Ok(()))
    }

    fn subscribe(&self) -> impl Future<Output = Result<Self::Incoming, Self::Error>> + Send {
        ready(Ok(self
            .state
            .incoming
            .lock()
            .expect("memory transport lock poisoned")
            .take()
            .expect("memory transport subscribed twice")))
    }
}
