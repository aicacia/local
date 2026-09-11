use axum::{
    Json,
    extract::{Path, Query, State},
};
use idp_model::contract::{ErrorResponse, UserInfo};
use serde::{Deserialize, Serialize};

use crate::router::{RouterState, middleware::ManagementAuthorization};

use super::roles::require_application_permission;

const USERS_READ_PERMISSION: &str = "users.read";

#[derive(
    Clone, Debug, Deserialize, Eq, PartialEq, Serialize, utoipa::IntoParams, utoipa::ToSchema,
)]
pub(crate) struct ListUsersQuery {
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, utoipa::ToSchema)]
pub(crate) struct UserApplicationRoleResponse {
    pub role_id: i64,
    pub application_id: i64,
    pub role_name: String,
    pub role_description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<idp_model::model::Role> for UserApplicationRoleResponse {
    fn from(value: idp_model::model::Role) -> Self {
        Self {
            role_id: value.id,
            application_id: value.application_id,
            role_name: value.name,
            role_description: value.description,
            created_at: value.created_at.timestamp(),
            updated_at: value.updated_at.timestamp(),
        }
    }
}

#[utoipa::path(
    get,
    path = "/users",
    params(ListUsersQuery),
    responses((status = 200, description = "List users", body = [UserInfo])),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn list_users(
    State(state): State<RouterState>,
    Query(query): Query<ListUsersQuery>,
    authorization: ManagementAuthorization,
) -> Result<Json<Vec<UserInfo>>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        USERS_READ_PERMISSION,
    )
    .await?;

    let users = state
        .oauth2_service
        .list_user_info(query.offset, normalize_limit(query.limit))
        .await?;

    Ok(Json(users))
}

#[utoipa::path(
    get,
    path = "/users/{user_id}",
    params(
        ("user_id" = i64, Path, description = "User ID")
    ),
    responses((status = 200, description = "Get user", body = UserInfo)),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn get_user(
    State(state): State<RouterState>,
    Path(user_id): Path<i64>,
    authorization: ManagementAuthorization,
) -> Result<Json<UserInfo>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        USERS_READ_PERMISSION,
    )
    .await?;

    let user = state.oauth2_service.find_user_info(user_id).await?;
    Ok(Json(user))
}

#[utoipa::path(
    get,
    path = "/users/{user_id}/roles",
    params(
        ("user_id" = i64, Path, description = "User ID")
    ),
    responses((status = 200, description = "List user roles across applications", body = [UserApplicationRoleResponse])),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn list_user_roles_across_applications(
    State(state): State<RouterState>,
    Path(user_id): Path<i64>,
    authorization: ManagementAuthorization,
) -> Result<Json<Vec<UserApplicationRoleResponse>>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        USERS_READ_PERMISSION,
    )
    .await?;

    let roles = state
        .management_service
        .list_user_roles_across_applications(user_id)
        .await
        .map_err(ErrorResponse::from)?;

    Ok(Json(roles.into_iter().map(Into::into).collect()))
}

const fn default_limit() -> u32 {
    50
}

fn normalize_limit(limit: u32) -> u32 {
    limit.clamp(1, 100)
}
