use alloc::{sync::Arc, vec::Vec};
use core::{
    convert::Infallible,
    future::{Future, ready},
    pin::Pin,
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

pub struct MemoryTransport<P> {
    peer: P,
    peers: Vec<(P, Sender<P>)>,
    incoming: Mutex<Option<MemoryIncoming<P>>>,
    mutator: Option<MemoryTransportMutator>,
}

impl<P> MemoryTransport<P>
where
    P: Clone + Eq + Send + 'static,
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
        let left = Self {
            peer: left_peer.clone(),
            peers: vec![(right_peer.clone(), right_sender)],
            incoming: Mutex::new(Some(MemoryIncoming(left_receiver))),
            mutator: left_mutator,
        };
        let right = Self {
            peer: right_peer.clone(),
            peers: vec![(left_peer, left_sender)],
            incoming: Mutex::new(Some(MemoryIncoming(right_receiver))),
            mutator: right_mutator,
        };
        (left, right)
    }

    fn deliver(&self, peer: P, mut data: Vec<u8>) {
        if let Some(mutator) = &self.mutator {
            mutator(&mut data);
        }
        let sender = self
            .peers
            .iter()
            .find_map(|(candidate, sender)| (*candidate == peer).then_some(sender))
            .expect("unknown memory transport peer");
        let _ = sender.send((self.peer.clone(), data));
    }
}

impl<P> Transport for MemoryTransport<P>
where
    P: Clone + Eq + Send + 'static,
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
        for (peer, _) in &self.peers {
            self.deliver(peer.clone(), data.clone());
        }
        ready(Ok(()))
    }

    fn subscribe(&self) -> impl Future<Output = Result<Self::Incoming, Self::Error>> + Send {
        ready(Ok(self
            .incoming
            .lock()
            .expect("memory transport lock poisoned")
            .take()
            .expect("memory transport subscribed twice")))
    }
}
