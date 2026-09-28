use std::{future::Future, sync::Arc};

use axum::{
    Router,
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use storage_model::{StorageErrorCode, StorageRequest, StorageResponse, StorageSession};

pub struct StorageSocketAccess<S> {
    pub read: bool,
    pub write: bool,
    pub session: S,
}

pub trait StorageSocketAuthorizer: Send + Sync + 'static {
    type Session: StorageSession + 'static;

    fn authorize(
        &self,
        token: String,
        resource_id: String,
    ) -> impl Future<Output = Result<StorageSocketAccess<Self::Session>, ()>> + Send;
}

#[derive(serde::Deserialize)]
struct StorageQuery {
    access_token: String,
    filesystem_id: String,
}

pub fn storage_router<A>(authorizer: Arc<A>) -> Router
where
    A: StorageSocketAuthorizer,
{
    Router::new()
        .route("/storage", get(upgrade_socket::<A>))
        .with_state(authorizer)
}

async fn upgrade_socket<A>(
    upgrade: WebSocketUpgrade,
    Query(query): Query<StorageQuery>,
    State(authorizer): State<Arc<A>>,
) -> Response
where
    A: StorageSocketAuthorizer,
{
    let Ok(access) = authorizer
        .authorize(query.access_token, query.filesystem_id)
        .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    upgrade.on_upgrade(move |socket| serve_socket(socket, access))
}

async fn serve_socket<S>(mut socket: WebSocket, access: StorageSocketAccess<S>)
where
    S: StorageSession + 'static,
{
    while let Some(Ok(Message::Text(message))) = socket.recv().await {
        let response = match serde_json::from_str::<StorageRequest>(&message) {
            Ok(request) if allows(&access, &request) => {
                access.session.execute_session(request).await
            }
            Ok(_) => StorageResponse::Error {
                code: StorageErrorCode::OperationFailed,
            },
            Err(_) => StorageResponse::Error {
                code: StorageErrorCode::InvalidRequest,
            },
        };
        if send_response(&mut socket, response).await.is_err() {
            return;
        }
    }
}

fn allows<S>(access: &StorageSocketAccess<S>, request: &StorageRequest) -> bool {
    match request {
        StorageRequest::Read { .. }
        | StorageRequest::Entry { .. }
        | StorageRequest::List { .. } => access.read,
        StorageRequest::Write { .. }
        | StorageRequest::Append { .. }
        | StorageRequest::Delete { .. }
        | StorageRequest::CreateDir { .. }
        | StorageRequest::Rename { .. } => access.write,
    }
}

async fn send_response(socket: &mut WebSocket, response: StorageResponse) -> Result<(), ()> {
    let response = serde_json::to_string(&response).map_err(|_| ())?;
    socket
        .send(Message::Text(response.into()))
        .await
        .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{StorageSocketAccess, allows};
    use storage_model::{StorageRequest, StorageResponse, StorageSession};

    struct Session;

    impl StorageSession for Session {
        fn execute_session(
            &self,
            _: StorageRequest,
        ) -> impl std::future::Future<Output = StorageResponse> + Send {
            async { StorageResponse::Deleted }
        }
    }

    #[test]
    fn authorization_is_resource_wide_and_action_bounded() {
        let access = StorageSocketAccess {
            read: true,
            write: false,
            session: Session,
        };
        assert!(allows(
            &access,
            &StorageRequest::Read {
                path: "any/file".into()
            }
        ));
        assert!(!allows(
            &access,
            &StorageRequest::Write {
                path: "a/file".into(),
                content: Vec::new()
            }
        ));
        assert!(!allows(
            &access,
            &StorageRequest::Rename {
                from: "any/file".into(),
                to: "any/renamed".into()
            }
        ));
    }
}
