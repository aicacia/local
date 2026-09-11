use axum::{Json, extract::State};
use idp_model::contract::{AuthorizationServerMetadata, ErrorResponse, Jwks};

use crate::router::RouterState;

#[utoipa::path(get, path = "/.well-known/jwks.json", responses((status = 200, description = "JWKS", body = Jwks)))]
pub(crate) async fn jwks(State(state): State<RouterState>) -> Result<Json<Jwks>, ErrorResponse> {
    let jwks = state.oauth2_service.list_jwks().await?;
    Ok(Json(jwks))
}

#[utoipa::path(get, path = "/.well-known/openid-configuration", responses((status = 200, description = "OIDC configuration", body = AuthorizationServerMetadata)))]
pub(crate) async fn openid_configuration(
    State(state): State<RouterState>,
) -> Json<AuthorizationServerMetadata> {
    Json(state.oauth2_service.metadata())
}
