use axum::{
    Json,
    extract::{Path, Query, State},
};
use idp_model::contract::{ClientRegistration, ErrorResponse};
use serde::{Deserialize, Serialize};

use crate::router::{RouterState, middleware::ManagementAuthorization};

use super::roles::require_application_permission;

const CLIENTS_READ_PERMISSION: &str = "clients.read";
const CLIENTS_WRITE_PERMISSION: &str = "clients.write";

#[derive(
    Clone, Debug, Deserialize, Eq, PartialEq, Serialize, utoipa::IntoParams, utoipa::ToSchema,
)]
pub(crate) struct ListClientsQuery {
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

#[utoipa::path(
    get,
    path = "/clients",
    params(ListClientsQuery),
    responses((status = 200, description = "List clients", body = [ClientRegistration])),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn list_clients(
    State(state): State<RouterState>,
    Query(query): Query<ListClientsQuery>,
    authorization: ManagementAuthorization,
) -> Result<Json<Vec<ClientRegistration>>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        CLIENTS_READ_PERMISSION,
    )
    .await?;

    let mut clients = state
        .oauth2_service
        .list_clients(query.offset, normalize_limit(query.limit))
        .await?;
    clients.iter_mut().for_each(sanitize_client_secret);

    Ok(Json(clients))
}

#[utoipa::path(
    post,
    path = "/clients",
    request_body = ClientRegistration,
    responses((status = 201, description = "Create client", body = ClientRegistration)),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn create_client(
    State(state): State<RouterState>,
    authorization: ManagementAuthorization,
    Json(body): Json<ClientRegistration>,
) -> Result<Json<ClientRegistration>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        CLIENTS_WRITE_PERMISSION,
    )
    .await?;

    let response = state.oauth2_service.register_client(body).await?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/clients/{client_id}",
    params(
        ("client_id" = String, Path, description = "Client ID")
    ),
    responses((status = 200, description = "Get client", body = ClientRegistration)),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn get_client(
    State(state): State<RouterState>,
    Path(client_id): Path<String>,
    authorization: ManagementAuthorization,
) -> Result<Json<ClientRegistration>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        CLIENTS_READ_PERMISSION,
    )
    .await?;

    let mut response = state.oauth2_service.get_client(&client_id).await?;
    sanitize_client_secret(&mut response);
    Ok(Json(response))
}

#[utoipa::path(
    put,
    path = "/clients/{client_id}",
    params(
        ("client_id" = String, Path, description = "Client ID")
    ),
    request_body = ClientRegistration,
    responses((status = 200, description = "Update client", body = ClientRegistration)),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn update_client(
    State(state): State<RouterState>,
    Path(client_id): Path<String>,
    authorization: ManagementAuthorization,
    Json(body): Json<ClientRegistration>,
) -> Result<Json<ClientRegistration>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        CLIENTS_WRITE_PERMISSION,
    )
    .await?;

    let mut response = state.oauth2_service.update_client(&client_id, body).await?;
    sanitize_client_secret(&mut response);
    Ok(Json(response))
}

#[utoipa::path(
    delete,
    path = "/clients/{client_id}",
    params(
        ("client_id" = String, Path, description = "Client ID")
    ),
    responses((status = 204, description = "Delete client")),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn delete_client(
    State(state): State<RouterState>,
    Path(client_id): Path<String>,
    authorization: ManagementAuthorization,
) -> Result<(), ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        CLIENTS_WRITE_PERMISSION,
    )
    .await?;
    state.oauth2_service.delete_client(&client_id).await
}

const fn default_limit() -> u32 {
    50
}

fn normalize_limit(limit: u32) -> u32 {
    limit.clamp(1, 100)
}

fn sanitize_client_secret(client: &mut ClientRegistration) {
    client.client_secret = None;
}
