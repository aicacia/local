use axum::{
    Json,
    extract::{Path, Query, State},
};
use idp_model::contract::{ErrorCode, ErrorResponse};
use serde::{Deserialize, Serialize};

use crate::router::{RouterState, middleware::ManagementAuthorization};

use super::roles::require_application_permission;

const APPLICATIONS_READ_PERMISSION: &str = "applications.read";
const APPLICATIONS_WRITE_PERMISSION: &str = "applications.write";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, utoipa::ToSchema)]
pub(crate) struct ApplicationResponse {
    pub id: i64,
    pub name: String,
    pub uri: String,
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<idp_model::model::Application> for ApplicationResponse {
    fn from(value: idp_model::model::Application) -> Self {
        Self {
            id: value.id,
            name: value.name,
            uri: value.uri,
            description: value.description,
            created_at: value.created_at.timestamp(),
            updated_at: value.updated_at.timestamp(),
        }
    }
}

#[derive(
    Clone, Debug, Deserialize, Eq, PartialEq, Serialize, utoipa::IntoParams, utoipa::ToSchema,
)]
pub(crate) struct ListApplicationsQuery {
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, utoipa::ToSchema)]
pub(crate) struct CreateApplicationRequest {
    pub name: String,
    pub uri: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, utoipa::ToSchema)]
pub(crate) struct UpdateApplicationRequest {
    pub name: Option<String>,
    pub uri: Option<String>,
    pub description: Option<String>,
}

#[utoipa::path(
    get,
    path = "/applications",
    params(ListApplicationsQuery),
    responses((status = 200, description = "List applications", body = [ApplicationResponse])),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn list_applications(
    State(state): State<RouterState>,
    Query(query): Query<ListApplicationsQuery>,
    authorization: ManagementAuthorization,
) -> Result<Json<Vec<ApplicationResponse>>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        APPLICATIONS_READ_PERMISSION,
    )
    .await?;

    let applications = state
        .management_service
        .list_applications(query.offset, normalize_limit(query.limit))
        .await
        .map_err(ErrorResponse::from)?;

    Ok(Json(applications.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    post,
    path = "/applications",
    request_body = CreateApplicationRequest,
    responses((status = 201, description = "Create application", body = ApplicationResponse)),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn create_application(
    State(state): State<RouterState>,
    authorization: ManagementAuthorization,
    Json(body): Json<CreateApplicationRequest>,
) -> Result<Json<ApplicationResponse>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        APPLICATIONS_WRITE_PERMISSION,
    )
    .await?;

    let created = state
        .management_service
        .create_application(body.name, body.uri, body.description)
        .await
        .map_err(ErrorResponse::from)?;

    Ok(Json(created.into()))
}

#[utoipa::path(
    get,
    path = "/applications/{application_id}",
    params(
        ("application_id" = i64, Path, description = "Application ID (URI)")
    ),
    responses((status = 200, description = "Get application", body = ApplicationResponse)),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn get_application(
    State(state): State<RouterState>,
    Path(application_id): Path<i64>,
    authorization: ManagementAuthorization,
) -> Result<Json<ApplicationResponse>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        APPLICATIONS_READ_PERMISSION,
    )
    .await?;

    let application = state
        .management_service
        .find_application_by_id(application_id)
        .await
        .map_err(ErrorResponse::from)?
        .ok_or_else(|| {
            ErrorResponse::new(ErrorCode::NotFound).with_description("Application not found")
        })?;

    Ok(Json(application.into()))
}

#[utoipa::path(
    put,
    path = "/applications/{application_id}",
    params(
        ("application_id" = i64, Path, description = "Application ID (URI)")
    ),
    request_body = UpdateApplicationRequest,
    responses((status = 200, description = "Update application", body = ApplicationResponse)),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn update_application(
    State(state): State<RouterState>,
    Path(application_id): Path<i64>,
    authorization: ManagementAuthorization,
    Json(body): Json<UpdateApplicationRequest>,
) -> Result<Json<ApplicationResponse>, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        APPLICATIONS_WRITE_PERMISSION,
    )
    .await?;

    let mut existing = state
        .management_service
        .find_application_by_id(application_id)
        .await
        .map_err(ErrorResponse::from)?
        .ok_or_else(|| {
            ErrorResponse::new(ErrorCode::NotFound).with_description("Application not found")
        })?;

    if let Some(name) = body.name {
        existing.name = name;
    }
    if let Some(uri) = body.uri {
        existing.uri = uri;
    }
    if let Some(description) = body.description {
        existing.description = Some(description);
    }

    let updated = state
        .management_service
        .update_application(existing)
        .await
        .map_err(ErrorResponse::from)?;

    Ok(Json(updated.into()))
}

#[utoipa::path(
    delete,
    path = "/applications/{application_id}",
    params(
        ("application_id" = i64, Path, description = "Application ID (URI)")
    ),
    responses((status = 204, description = "Delete application")),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn delete_application(
    State(state): State<RouterState>,
    Path(application_id): Path<i64>,
    authorization: ManagementAuthorization,
) -> Result<(), ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        APPLICATIONS_WRITE_PERMISSION,
    )
    .await?;

    state
        .management_service
        .delete_application_by_id(application_id)
        .await
        .map_err(ErrorResponse::from)
}

const fn default_limit() -> u32 {
    50
}

fn normalize_limit(limit: u32) -> u32 {
    limit.clamp(1, 100)
}
