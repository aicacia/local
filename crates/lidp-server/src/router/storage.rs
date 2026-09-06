use std::{future::Future, pin::Pin, sync::Arc};

use axum::Router;
use lidp_service::{
    scoped_file_system::ScopedFileSystemRuntime, storage_session::StorageSessionService,
};
use storage_server::{
    StorageSessionResolver, StorageSocketSession, storage_router as socket_router,
};
use storage_service::StorageService;

pub fn storage_router(
    sessions: Arc<StorageSessionService>,
    file_systems: Arc<ScopedFileSystemRuntime>,
) -> Router {
    socket_router(Arc::new(ScopedFileSystemSessionResolver {
        sessions,
        file_systems,
    }))
}

struct ScopedFileSystemSessionResolver {
    sessions: Arc<StorageSessionService>,
    file_systems: Arc<ScopedFileSystemRuntime>,
}

impl StorageSessionResolver for ScopedFileSystemSessionResolver {
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
