use axum::{
    Json,
    extract::{Path, State},
};
use lidp_model::contract::{ClientRegistration, ErrorResponse};

use crate::router::{RouterState, middleware::StandardAuthorization};

#[utoipa::path(
    post,
    path = "/oauth2/register",
    request_body(
        content(
            (ClientRegistration = "application/json"),
            (ClientRegistration = "application/x-www-form-urlencoded")
        )
    ),
    responses((status = 201, description = "Register client", body = ClientRegistration)),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn register(
    State(state): State<RouterState>,
    StandardAuthorization { .. }: StandardAuthorization,
    Json(body): Json<ClientRegistration>,
) -> Result<Json<ClientRegistration>, ErrorResponse> {
    log::info!("Registering client: {:?}", body);
    let response = state.oauth2_service.register_client(body).await?;
    log::info!("Registered client: {:?}", response);
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/oauth2/register/{client_id}",
    params(
        ("client_id" = String, Path, description = "Client ID")
    ),
    responses((status = 200, description = "Get client", body = ClientRegistration))
)]
pub(crate) async fn get_register(
    State(state): State<RouterState>,
    Path(client_id): Path<String>,
) -> Result<Json<ClientRegistration>, ErrorResponse> {
    let mut response = state.oauth2_service.get_client(&client_id).await?;
    // Do not return the client secret in the response
    response.client_secret = None;
    Ok(Json(response))
}

#[utoipa::path(
    delete,
    path = "/oauth2/register/{client_id}",
    params(
        ("client_id" = String, Path, description = "Client ID")
    ),
    responses((status = 204, description = "Delete client")),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn delete_register(
    State(state): State<RouterState>,
    Path(client_id): Path<String>,
    StandardAuthorization { .. }: StandardAuthorization,
) -> Result<(), ErrorResponse> {
    state.oauth2_service.delete_client(&client_id).await
}

#[utoipa::path(
    put,
    path = "/oauth2/register/{client_id}",
    params(
        ("client_id" = String, Path, description = "Client ID")
    ),
    request_body(
        content(
            (ClientRegistration = "application/json"),
            (ClientRegistration = "application/x-www-form-urlencoded")
        )
    ),
    responses((status = 200, description = "Update client", body = ClientRegistration)),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn put_register(
    State(state): State<RouterState>,
    Path(client_id): Path<String>,
    StandardAuthorization { .. }: StandardAuthorization,
    Json(body): Json<ClientRegistration>,
) -> Result<Json<ClientRegistration>, ErrorResponse> {
    let response = state.oauth2_service.update_client(&client_id, body).await?;
    Ok(Json(response))
}
