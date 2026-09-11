use axum::{
    Json,
    extract::{Path, Query, State},
};
use idp_model::contract::{ErrorCode, ErrorResponse};
use serde::{Deserialize, Serialize};

use crate::router::{RouterState, middleware::ManagementAuthorization};

use super::roles::require_application_permission;

const CONSENTS_READ_PERMISSION: &str = "consents.read";
const CONSENTS_WRITE_PERMISSION: &str = "consents.write";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, utoipa::ToSchema)]
pub(crate) struct UserConsentResponse {
    pub id: i64,
    pub user_id: i64,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<idp_model::model::OAuth2UserConsent> for UserConsentResponse {
    fn from(value: idp_model::model::OAuth2UserConsent) -> Self {
        Self {
            id: value.id,
            user_id: value.user_id,
            client_id: value.client_id,
            redirect_uri: value.redirect_uri,
            scope: value.scope,
            created_at: value.created_at.timestamp(),
            updated_at: value.updated_at.timestamp(),
        }
    }
}

#[derive(
    Clone, Debug, Deserialize, Eq, PartialEq, Serialize, utoipa::IntoParams, utoipa::ToSchema,
)]
pub(crate) struct ListUserConsentsQuery {
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

#[utoipa::path(
    get,
    path = "/users/{user_id}/consents",
    params(
        ("user_id" = i64, Path, description = "User ID"),
        ListUserConsentsQuery
    ),
    responses((status = 200, description = "List user consents", body = [UserConsentResponse])),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn list_user_consents(
    State(state): State<RouterState>,
    Path(user_id): Path<i64>,
    Query(query): Query<ListUserConsentsQuery>,
    authorization: ManagementAuthorization,
) -> Result<Json<Vec<UserConsentResponse>>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        CONSENTS_READ_PERMISSION,
    )
    .await?;

    let consents = state
        .oauth2_service
        .list_user_consents(user_id, query.offset, normalize_limit(query.limit))
        .await?;

    Ok(Json(consents.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    delete,
    path = "/users/{user_id}/consents/{consent_id}",
    params(
        ("user_id" = i64, Path, description = "User ID"),
        ("consent_id" = i64, Path, description = "Consent ID")
    ),
    responses((status = 204, description = "Revoke user consent")),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn revoke_user_consent(
    State(state): State<RouterState>,
    Path((user_id, consent_id)): Path<(i64, i64)>,
    authorization: ManagementAuthorization,
) -> Result<(), ErrorResponse> {
    let _consent = state
        .oauth2_service
        .list_user_consents(user_id, 0, 1_000)
        .await?
        .into_iter()
        .find(|item| item.id == consent_id)
        .ok_or_else(|| {
            ErrorResponse::new(ErrorCode::NotFound).with_description("User consent not found")
        })?;

    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        CONSENTS_WRITE_PERMISSION,
    )
    .await?;

    state
        .oauth2_service
        .revoke_user_consent(user_id, consent_id)
        .await
}

const fn default_limit() -> u32 {
    50
}

fn normalize_limit(limit: u32) -> u32 {
    limit.clamp(1, 100)
}
