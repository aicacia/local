use axum::{
    Json,
    extract::{Path, State},
};
use idp_model::{
    contract::{EntityType, ErrorResponse, JwkPublic},
    model::Key,
};
use serde::{Deserialize, Serialize};

use crate::router::{RouterState, middleware::ManagementAuthorization};

use super::roles::require_application_permission;

const KEYS_READ_PERMISSION: &str = "keys.read";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, utoipa::ToSchema)]
pub(crate) struct ManagementKey {
    pub id: u32,
    pub parent_id: Option<u32>,
    pub entity_type: EntityType,
    pub entity_id: idp_model::model::Id,
    pub derivation_path: String,
    pub name: String,
    pub hardened: bool,
    pub revoked_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<Key> for ManagementKey {
    fn from(key: Key) -> Self {
        Self {
            id: key.id,
            parent_id: key.parent_id,
            entity_type: key.entity_type,
            entity_id: key.entity_id,
            derivation_path: key.derivation_path,
            name: key.name,
            hardened: key.hardened,
            revoked_at: key.revoked_at.map(|value| value.timestamp()),
            expires_at: key.expires_at.map(|value| value.timestamp()),
            created_at: key.created_at.timestamp(),
            updated_at: key.updated_at.timestamp(),
        }
    }
}

#[utoipa::path(
    get,
    path = "/clients/{client_id}/keys",
    params(
        ("client_id" = String, Path, description = "Client ID")
    ),
    responses((status = 200, description = "List client keys", body = [ManagementKey])),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn list_client_keys(
    State(state): State<RouterState>,
    Path(client_id): Path<String>,
    authorization: ManagementAuthorization,
) -> Result<Json<Vec<ManagementKey>>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        KEYS_READ_PERMISSION,
    )
    .await?;

    let keys = state.oauth2_service.list_client_keys(&client_id).await?;
    Ok(Json(keys.into_iter().map(ManagementKey::from).collect()))
}

#[utoipa::path(
    get,
    path = "/keys/{key_id}/jwk",
    params(
        ("key_id" = u32, Path, description = "Key ID")
    ),
    responses((status = 200, description = "Get public JWK for key", body = JwkPublic)),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn get_key_jwk(
    State(state): State<RouterState>,
    Path(key_id): Path<u32>,
    authorization: ManagementAuthorization,
) -> Result<Json<JwkPublic>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        KEYS_READ_PERMISSION,
    )
    .await?;

    let jwk = state.oauth2_service.find_public_jwk(key_id).await?;
    Ok(Json(jwk))
}
