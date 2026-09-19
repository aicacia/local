use std::sync::Arc;

use axum::Router;
use iroh::EndpointId;
use model::contract::{AuthorizationDetail, StandardClaims, StorageAuthorizationAction};
use storage_server::{
    StorageSocketAccess, StorageSocketAuthorizer, storage_router as socket_router,
};
use storage_service::{ScopedFileSystemRuntime, ScopedStorageService};

use crate::RouterState;

pub fn storage_router(
    state: RouterState,
    file_systems: Arc<ScopedFileSystemRuntime<EndpointId>>,
) -> Router {
    socket_router(Arc::new(ScopedFileSystemSocketAuthorizer {
        state,
        file_systems,
    }))
}

struct ScopedFileSystemSocketAuthorizer {
    state: RouterState,
    file_systems: Arc<ScopedFileSystemRuntime<EndpointId>>,
}

impl StorageSocketAuthorizer for ScopedFileSystemSocketAuthorizer {
    type Session = ScopedStorageService<EndpointId, StorageNamespace>;

    async fn authorize(&self, token: String) -> Result<StorageSocketAccess<Self::Session>, ()> {
        let authorization = super::middleware::authorize_bearer(&self.state, &token)
            .await
            .map_err(|_| ())?;
        let (folder, write) = storage_access(&authorization.claims).ok_or(())?;
        let client_id = &authorization.claims.client_id;
        let application_id = self
            .state
            .oauth2_service
            .application_id_for_client(client_id)
            .await
            .map_err(|_| ())?;
        let scope = StorageNamespace {
            user_sub: authorization.claims.sub,
            application_id,
        };
        let session = ScopedStorageService::open(&self.file_systems, scope)
            .await
            .map_err(|_| ())?;
        Ok(StorageSocketAccess {
            folder,
            write,
            session,
        })
    }
}

fn storage_access(claims: &StandardClaims) -> Option<(String, bool)> {
    let resource = claims.resource.as_deref()?;
    if claims.aud != resource {
        return None;
    }
    let [AuthorizationDetail::Storage(detail)] = claims.authorization_details.as_deref()? else {
        return None;
    };
    let write = detail.actions.contains(&StorageAuthorizationAction::Write);
    detail
        .actions
        .contains(&StorageAuthorizationAction::Read)
        .then(|| (detail.folder.clone(), write))
}

struct StorageNamespace {
    user_sub: String,
    application_id: i64,
}

impl storage_model::StorageNamespace for StorageNamespace {
    fn user_sub(&self) -> &str {
        &self.user_sub
    }

    fn application_id(&self) -> i64 {
        self.application_id
    }
}
