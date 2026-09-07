use std::{future::Future, pin::Pin, sync::Arc};

use axum::Router;
use file_system::{PeerCodec, Transport};
use lidp_service::{
    scoped_file_system::{ScopedFileSystemRuntime, ScopedTransportFactory},
    storage_session::StorageSessionService,
};
use storage_server::{
    StorageSessionResolver, StorageSocketSession, storage_router as socket_router,
};
use storage_service::StorageService;

pub fn storage_router<C, T, F>(
    sessions: Arc<StorageSessionService>,
    file_systems: Arc<ScopedFileSystemRuntime<C, T, F>>,
) -> Router
where
    C: PeerCodec + Send + Sync + 'static,
    C::Error: Send + Sync + 'static,
    C::PeerId: Clone + Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Error: std::fmt::Display + Send + Sync + 'static,
    T::Incoming: Send + 'static,
    F: ScopedTransportFactory<C, T>,
{
    socket_router(Arc::new(ScopedFileSystemSessionResolver {
        sessions,
        file_systems,
    }))
}

struct ScopedFileSystemSessionResolver<C, T, F>
where
    C: PeerCodec,
    T: Transport<PeerId = C::PeerId>,
    F: ScopedTransportFactory<C, T>,
{
    sessions: Arc<StorageSessionService>,
    file_systems: Arc<ScopedFileSystemRuntime<C, T, F>>,
}

impl<C, T, F> StorageSessionResolver for ScopedFileSystemSessionResolver<C, T, F>
where
    C: PeerCodec + Send + Sync + 'static,
    C::Error: Send + Sync + 'static,
    C::PeerId: Clone + Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Error: std::fmt::Display + Send + Sync + 'static,
    T::Incoming: Send + 'static,
    F: ScopedTransportFactory<C, T>,
{
    fn take(
        &self,
        token: &str,
    ) -> Pin<Box<dyn Future<Output = Option<Arc<dyn StorageSocketSession>>> + Send + '_>> {
        let scope = self.sessions.take(token);
        let file_systems = self.file_systems.clone();
        Box::pin(async move {
            let scope = scope?;
            let file_system = file_systems.open(&scope).await.ok()?;
            let session: Arc<dyn StorageSocketSession> = Arc::new(StorageService::new(file_system));
            Some(session)
        })
    }
}
