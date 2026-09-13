use std::{future::Future, pin::Pin, sync::Arc};

use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
    routing::get,
};
use file_system::{PeerCodec, Storage, Transport};
use storage_model::{StorageErrorCode, StorageResponse, StorageSocketRequest};
use storage_service::StorageService;

type StorageSessionFuture<'a> =
    Pin<Box<dyn Future<Output = Option<Arc<dyn StorageSocketSession>>> + Send + 'a>>;

pub trait StorageSocketSession: Send + Sync {
    fn execute(
        &self,
        request: storage_model::StorageRequest,
    ) -> Pin<Box<dyn Future<Output = StorageResponse> + Send + '_>>;
}

pub trait StorageSessionResolver: Send + Sync + 'static {
    fn take(&self, token: &str) -> StorageSessionFuture<'_>;
}

impl<S, C, T> StorageSocketSession for StorageService<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: Send + Sync + 'static,
    C: PeerCodec + Send + Sync + 'static,
    C::Error: Send + Sync + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: Send + 'static,
{
    fn execute(
        &self,
        request: storage_model::StorageRequest,
    ) -> Pin<Box<dyn Future<Output = StorageResponse> + Send + '_>> {
        Box::pin(async move {
            self.execute(request)
                .await
                .unwrap_or(StorageResponse::Error {
                    code: StorageErrorCode::OperationFailed,
                })
        })
    }
}

pub fn storage_router(resolver: Arc<dyn StorageSessionResolver>) -> Router {
    Router::new()
        .route("/storage", get(upgrade_socket))
        .with_state(resolver)
}

async fn upgrade_socket(
    upgrade: WebSocketUpgrade,
    State(resolver): State<Arc<dyn StorageSessionResolver>>,
) -> Response {
    upgrade.on_upgrade(move |socket| serve_socket(socket, resolver))
}

async fn serve_socket(mut socket: WebSocket, resolver: Arc<dyn StorageSessionResolver>) {
    let Some(Ok(Message::Text(message))) = socket.recv().await else {
        return;
    };
    let Ok(StorageSocketRequest::Authenticate { token }) = serde_json::from_str(&message) else {
        let _ = send_response(
            &mut socket,
            StorageResponse::Error {
                code: StorageErrorCode::InvalidRequest,
            },
        )
        .await;
        return;
    };
    let Some(session) = resolver.take(&token).await else {
        let _ = send_response(
            &mut socket,
            StorageResponse::Error {
                code: StorageErrorCode::OperationFailed,
            },
        )
        .await;
        return;
    };
    if send_response(&mut socket, StorageResponse::Authenticated)
        .await
        .is_err()
    {
        return;
    }
    while let Some(Ok(message)) = socket.recv().await {
        let Message::Text(message) = message else {
            continue;
        };
        let response = match serde_json::from_str::<StorageSocketRequest>(&message) {
            Ok(StorageSocketRequest::Request { request }) => session.execute(request).await,
            Err(_) => StorageResponse::Error {
                code: StorageErrorCode::InvalidRequest,
            },
            Ok(StorageSocketRequest::Authenticate { .. }) => StorageResponse::Error {
                code: StorageErrorCode::InvalidRequest,
            },
        };
        if send_response(&mut socket, response).await.is_err() {
            return;
        }
    }
}

async fn send_response(socket: &mut WebSocket, response: StorageResponse) -> Result<(), ()> {
    let response = serde_json::to_string(&response).map_err(|_| ())?;
    socket
        .send(Message::Text(response.into()))
        .await
        .map_err(|_| ())
}
