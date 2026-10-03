use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    routing::{get, post},
};
use idp_model::model::Id;
use iroh::EndpointId;
use serde::{Deserialize, Serialize};
use storage_service::{
    DatabaseId, DatabaseResource, FileSystemId, FileSystemResource, ScopedFileSystemRuntime,
};

use crate::{
    RouterState, StorageAuthorization, authorize_storage_token, scoped_file_system_socket_router,
};

#[derive(Clone)]
pub(super) struct ResourceState {
    pub router: RouterState,
    pub databases: Option<Arc<storage_service::DatabaseRuntime>>,
    pub file_systems: Option<Arc<ScopedFileSystemRuntime<EndpointId>>>,
}

#[derive(Debug, Eq, PartialEq)]
struct StorageNamespace {
    user_sub: String,
    application_id: Id,
}

impl storage_model::StorageNamespace for StorageNamespace {
    fn user_sub(&self) -> &str {
        &self.user_sub
    }

    fn application_id(&self) -> Id {
        self.application_id
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct CreateResourceRequest {
    name: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct FileSystemResourceResponse {
    id: String,
    name: Option<String>,
}

impl From<FileSystemResource> for FileSystemResourceResponse {
    fn from(resource: FileSystemResource) -> Self {
        Self {
            id: resource.id.as_uuid().to_string(),
            name: resource.name,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct DatabaseDetailResponse {
    #[schema(value_type = String)]
    id: DatabaseId,
    name: Option<String>,
    application_id: String,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct FileSystemDetailResponse {
    id: String,
    name: Option<String>,
    application_id: String,
}

pub fn resource_router(
    router: RouterState,
    databases: Option<Arc<storage_service::DatabaseRuntime>>,
    file_systems: Option<Arc<ScopedFileSystemRuntime<EndpointId>>>,
) -> Router {
    let state = ResourceState {
        router,
        databases,
        file_systems,
    };
    let mut router = Router::new()
        .route(
            "/storage/databases",
            post(create_database).get(list_databases),
        )
        .route(
            "/storage/databases/{database_id}",
            get(get_database).delete(delete_database),
        )
        .route(
            "/storage/filesystems",
            post(create_file_system).get(list_file_systems),
        )
        .route(
            "/storage/filesystems/{filesystem_id}",
            get(get_file_system).delete(delete_file_system),
        )
        .with_state(state.clone());
    if let (Some(idp_client), Some(file_systems)) = (state.router.idp_client, state.file_systems) {
        router = router.merge(scoped_file_system_socket_router(idp_client, file_systems));
    }
    router
}

async fn authorization(
    state: &ResourceState,
    headers: &HeaderMap,
) -> Result<StorageAuthorization, StatusCode> {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|token| !token.is_empty())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let idp_client = state
        .router
        .idp_client
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    authorize_storage_token(idp_client, token)
        .await
        .map_err(|error| match error {
            crate::StorageAuthorizationError::InvalidToken => StatusCode::UNAUTHORIZED,
            crate::StorageAuthorizationError::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        })
}

fn namespace(
    authorization: StorageAuthorization,
) -> Result<(StorageNamespace, bool, bool), StatusCode> {
    let application_id = authorization
        .application_id
        .parse::<Id>()
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok((
        StorageNamespace {
            user_sub: authorization.subject,
            application_id,
        },
        authorization.can_read,
        authorization.can_write,
    ))
}

#[utoipa::path(
    post,
    path = "/storage/databases",
    request_body = CreateResourceRequest,
    responses(
        (status = 201, description = "Database resource created", body = DatabaseResource),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit writes"),
        (status = 503, description = "Storage runtime or IdP unavailable")
    ),
    security(("authorization" = []))
)]
pub(super) async fn create_database(
    State(state): State<ResourceState>,
    headers: HeaderMap,
    Json(request): Json<CreateResourceRequest>,
) -> Result<(StatusCode, Json<DatabaseResource>), StatusCode> {
    let (scope, _, can_write) = namespace(authorization(&state, &headers).await?)?;
    if !can_write {
        return Err(StatusCode::FORBIDDEN);
    }
    let (resource, _) = state
        .databases
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .create(&scope, request.name)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(resource)))
}

#[utoipa::path(
    get,
    path = "/storage/databases",
    responses(
        (status = 200, description = "Namespace database resources", body = [DatabaseResource]),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit reads"),
        (status = 503, description = "Storage runtime or IdP unavailable")
    ),
    security(("authorization" = []))
)]
pub(super) async fn list_databases(
    State(state): State<ResourceState>,
    headers: HeaderMap,
) -> Result<Json<Vec<DatabaseResource>>, StatusCode> {
    let (scope, can_read, _) = namespace(authorization(&state, &headers).await?)?;
    if !can_read {
        return Err(StatusCode::FORBIDDEN);
    }
    state
        .databases
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .list(&scope)
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[utoipa::path(
    get,
    path = "/storage/databases/{database_id}",
    params(("database_id" = String, Path, description = "Database resource ID")),
    responses(
        (status = 200, description = "Database resource", body = DatabaseDetailResponse),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit reads"),
        (status = 404, description = "Database resource not found"),
        (status = 503, description = "Storage runtime or IdP unavailable")
    ),
    security(("authorization" = []))
)]
pub(super) async fn get_database(
    State(state): State<ResourceState>,
    headers: HeaderMap,
    Path(database_id): Path<DatabaseId>,
) -> Result<Json<DatabaseDetailResponse>, StatusCode> {
    let (scope, can_read, _) = namespace(authorization(&state, &headers).await?)?;
    if !can_read {
        return Err(StatusCode::FORBIDDEN);
    }
    state
        .databases
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .get(&scope, database_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(|resource| {
            Json(DatabaseDetailResponse {
                id: resource.id,
                name: resource.name,
                application_id: scope.application_id.to_string(),
            })
        })
        .ok_or(StatusCode::NOT_FOUND)
}

#[utoipa::path(
    delete,
    path = "/storage/databases/{database_id}",
    params(("database_id" = String, Path, description = "Database resource ID")),
    responses(
        (status = 204, description = "Database resource tombstoned"),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit writes"),
        (status = 404, description = "Database resource not found"),
        (status = 503, description = "Storage runtime or IdP unavailable")
    ),
    security(("authorization" = []))
)]
pub(super) async fn delete_database(
    State(state): State<ResourceState>,
    headers: HeaderMap,
    Path(database_id): Path<DatabaseId>,
) -> Result<StatusCode, StatusCode> {
    let (scope, _, can_write) = namespace(authorization(&state, &headers).await?)?;
    if !can_write {
        return Err(StatusCode::FORBIDDEN);
    }
    if state
        .databases
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .delete(&scope, database_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[utoipa::path(
    post,
    path = "/storage/filesystems",
    request_body = CreateResourceRequest,
    responses(
        (status = 201, description = "Filesystem resource created", body = FileSystemResourceResponse),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit writes"),
        (status = 503, description = "Storage runtime or IdP unavailable")
    ),
    security(("authorization" = []))
)]
pub(super) async fn create_file_system(
    State(state): State<ResourceState>,
    headers: HeaderMap,
    Json(request): Json<CreateResourceRequest>,
) -> Result<(StatusCode, Json<FileSystemResourceResponse>), StatusCode> {
    let (scope, _, can_write) = namespace(authorization(&state, &headers).await?)?;
    if !can_write {
        return Err(StatusCode::FORBIDDEN);
    }
    let resource = state
        .file_systems
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .create_resource(&scope, request.name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(resource.into())))
}

#[utoipa::path(
    get,
    path = "/storage/filesystems",
    responses(
        (status = 200, description = "Namespace filesystem resources", body = [FileSystemResourceResponse]),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit reads"),
        (status = 503, description = "Storage runtime or IdP unavailable")
    ),
    security(("authorization" = []))
)]
pub(super) async fn list_file_systems(
    State(state): State<ResourceState>,
    headers: HeaderMap,
) -> Result<Json<Vec<FileSystemResourceResponse>>, StatusCode> {
    let (scope, can_read, _) = namespace(authorization(&state, &headers).await?)?;
    if !can_read {
        return Err(StatusCode::FORBIDDEN);
    }
    state
        .file_systems
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .list_resources(&scope)
        .await
        .map(|resources| Json(resources.into_iter().map(Into::into).collect()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[utoipa::path(
    get,
    path = "/storage/filesystems/{filesystem_id}",
    params(("filesystem_id" = String, Path, description = "Filesystem resource ID")),
    responses(
        (status = 200, description = "Filesystem resource", body = FileSystemDetailResponse),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit reads"),
        (status = 404, description = "Filesystem resource not found"),
        (status = 503, description = "Storage runtime or IdP unavailable")
    ),
    security(("authorization" = []))
)]
pub(super) async fn get_file_system(
    State(state): State<ResourceState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<FileSystemDetailResponse>, StatusCode> {
    let (scope, can_read, _) = namespace(authorization(&state, &headers).await?)?;
    if !can_read {
        return Err(StatusCode::FORBIDDEN);
    }
    let id = FileSystemId::parse(&id).map_err(|_| StatusCode::NOT_FOUND)?;
    state
        .file_systems
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .list_resources(&scope)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .find(|resource| resource.id == id)
        .map(|resource| {
            Json(FileSystemDetailResponse {
                id: resource.id.as_uuid().to_string(),
                name: resource.name,
                application_id: scope.application_id.to_string(),
            })
        })
        .ok_or(StatusCode::NOT_FOUND)
}

#[utoipa::path(
    delete,
    path = "/storage/filesystems/{filesystem_id}",
    params(("filesystem_id" = String, Path, description = "Filesystem resource ID")),
    responses(
        (status = 204, description = "Filesystem resource tombstoned"),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit writes"),
        (status = 404, description = "Filesystem resource not found"),
        (status = 503, description = "Storage runtime or IdP unavailable")
    ),
    security(("authorization" = []))
)]
pub(super) async fn delete_file_system(
    State(state): State<ResourceState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let (scope, _, can_write) = namespace(authorization(&state, &headers).await?)?;
    if !can_write {
        return Err(StatusCode::FORBIDDEN);
    }
    let id = FileSystemId::parse(&id).map_err(|_| StatusCode::NOT_FOUND)?;
    if state
        .file_systems
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .delete_resource(&scope, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode, header::AUTHORIZATION},
    };
    use tower::ServiceExt;

    use crate::{RouterState, resource_router};

    fn router() -> axum::Router {
        resource_router(RouterState::new("http://storage.local"), None, None)
    }

    #[tokio::test]
    async fn routes_work_under_a_host_prefix_without_repeating_storage_prefix() {
        let response = axum::Router::new()
            .nest("/host", router())
            .oneshot(
                Request::builder()
                    .uri("/host/storage/databases")
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("request must complete");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rejects_missing_bearer_token() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/storage/databases")
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("request must complete");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn fails_closed_when_idp_client_is_not_configured() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/storage/databases")
                    .header(AUTHORIZATION, "Bearer token")
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("request must complete");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
