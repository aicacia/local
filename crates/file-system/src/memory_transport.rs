use alloc::{sync::Arc, vec::Vec};
use core::{
    convert::Infallible,
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
    task::{Context, Poll},
};

use futures_core::Stream;
use tokio::sync::{Mutex, mpsc};

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
    async fn receive(&self, message: Message<P>) {
        if self.online.load(Ordering::Acquire) {
            let _ = self.sender.send(message);
        } else {
            self.pending.lock().await.push(message);
        }
    }

    async fn set_online(&self, online: bool) {
        self.online.store(online, Ordering::Release);
        if !online {
            return;
        }
        let pending = core::mem::take(&mut *self.pending.lock().await);
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
    duplicate_next_outbound: AtomicBool,
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
                duplicate_next_outbound: AtomicBool::new(false),
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
                duplicate_next_outbound: AtomicBool::new(false),
            }),
        };
        (left, right)
    }

    pub async fn set_online(&self, online: bool) {
        self.state.endpoint.set_online(online).await;
        if !online {
            return;
        }
        let outbound = core::mem::take(&mut *self.state.outbound.lock().await);
        for (peer, data) in outbound {
            self.deliver(peer, data).await;
        }
    }

    #[must_use]
    pub fn is_online(&self) -> bool {
        self.state.endpoint.online.load(Ordering::Acquire)
    }

    pub fn duplicate_next_outbound(&self) {
        self.state
            .duplicate_next_outbound
            .store(true, Ordering::Release);
    }

    async fn deliver(&self, peer: P, mut data: Vec<u8>) {
        if !self.is_online() {
            self.state.outbound.lock().await.push((peer, data));
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

        endpoint.receive((self.peer.clone(), data.clone())).await;
        if self
            .state
            .duplicate_next_outbound
            .swap(false, Ordering::AcqRel)
        {
            endpoint.receive((self.peer.clone(), data)).await;
        }
    }
}

impl<P> Transport for MemoryTransport<P>
where
    P: Clone + Eq + Send + Sync + 'static,
{
    type Error = Infallible;
    type PeerId = P;
    type Incoming = MemoryIncoming<P>;

    async fn send(&self, peer: Self::PeerId, data: Vec<u8>) -> Result<(), Self::Error> {
        self.deliver(peer, data).await;
        Ok(())
    }

    async fn broadcast(&self, data: Vec<u8>) -> Result<(), Self::Error> {
        for (peer, _) in &self.state.peers {
            self.deliver(peer.clone(), data.clone()).await;
        }
        Ok(())
    }

    fn peers(&self) -> Vec<Self::PeerId> {
        self.state
            .peers
            .iter()
            .filter(|(_, endpoint)| endpoint.online.load(Ordering::Acquire))
            .map(|(peer, _)| peer.clone())
            .collect()
    }

    async fn subscribe(&self) -> Result<Self::Incoming, Self::Error> {
        Ok(self
            .state
            .incoming
            .lock()
            .await
            .take()
            .expect("memory transport subscribed twice"))
    }
}
