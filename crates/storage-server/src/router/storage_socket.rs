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
    pub folder: String,
    pub write: bool,
    pub session: S,
}

pub trait StorageSocketAuthorizer: Send + Sync + 'static {
    type Session: StorageSession + 'static;

    fn authorize(
        &self,
        token: String,
    ) -> impl Future<Output = Result<StorageSocketAccess<Self::Session>, ()>> + Send;
}

#[derive(serde::Deserialize)]
struct StorageQuery {
    access_token: String,
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
    let Ok(access) = authorizer.authorize(query.access_token).await else {
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
        StorageRequest::Read { path }
        | StorageRequest::Entry { path }
        | StorageRequest::List { path } => in_folder(&access.folder, path),
        StorageRequest::Write { path, .. }
        | StorageRequest::Append { path, .. }
        | StorageRequest::Delete { path }
        | StorageRequest::CreateDir { path } => access.write && in_folder(&access.folder, path),
        StorageRequest::Rename { from, to } => {
            access.write && in_folder(&access.folder, from) && in_folder(&access.folder, to)
        }
    }
}

fn in_folder(folder: &str, path: &str) -> bool {
    folder.is_empty()
        || path == folder
        || path
            .strip_prefix(folder)
            .is_some_and(|suffix| suffix.starts_with('/'))
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
    fn authorization_is_folder_bounded_and_read_only() {
        let access = StorageSocketAccess {
            folder: "a".into(),
            write: false,
            session: Session,
        };
        assert!(allows(
            &access,
            &StorageRequest::Read {
                path: "a/file".into()
            }
        ));
        assert!(!allows(
            &access,
            &StorageRequest::Read {
                path: "ab/file".into()
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
                from: "a/file".into(),
                to: "b/file".into()
            }
        ));
    }
}
