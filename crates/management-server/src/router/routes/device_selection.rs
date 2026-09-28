use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use idp_model::{
    contract::{DeviceState, ErrorCode, ErrorResponse},
    model::Id,
};
use management_service::{DeviceRepo, replica::SelectionPolicy};
use serde::Deserialize;
use storage_model::ResourceKind;

use crate::router::{RouterState, middleware::ManagementAuthorization};

use super::roles::require_application_permission;

const SELECTION_PERMISSION: &str = "devices.select";
const RESTRICT_PERMISSION: &str = "devices.restrict";
const STORAGE_AUTHORIZATION: &str = "x-storage-authorization";

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SelectionRequest {
    #[schema(value_type = String)]
    application_id: Id,
    kind: SelectionKind,
    #[schema(value_type = String)]
    id: Id,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SelectionKind {
    Database,
    Filesystem,
}

impl SelectionKind {
    fn resource_kind(&self) -> ResourceKind {
        match self {
            Self::Database => ResourceKind::Database,
            Self::Filesystem => ResourceKind::FileSystem,
        }
    }

    fn policy_kind(&self) -> &'static str {
        match self {
            Self::Database => "database",
            Self::Filesystem => "filesystem",
        }
    }
}

fn storage_token(headers: &HeaderMap) -> Result<&str, ErrorResponse> {
    let value = headers
        .get(STORAGE_AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty() && !value.trim().contains(char::is_whitespace))
        .ok_or_else(|| ErrorResponse::new(ErrorCode::NotAuthorized))?;
    Ok(value)
}

#[utoipa::path(
    put,
    path = "/devices/{device_id}/selection",
    params(("device_id" = String, Path, description = "Device ID")),
    request_body = SelectionRequest,
    responses((status = 204, description = "Device selection updated")),
    security(("authorization" = []))
)]
pub(crate) async fn put_device_selection(
    State(state): State<RouterState>,
    Path(device_id): Path<Id>,
    authorization: ManagementAuthorization,
    headers: HeaderMap,
    Json(body): Json<SelectionRequest>,
) -> Result<StatusCode, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        SELECTION_PERMISSION,
    )
    .await?;
    let owner = authorization.principal.get_entity_id().to_string();
    let token = storage_token(&headers)?;

    state
        .control_plane
        .validate_selection_device(token, &owner, &state.storage_audience, device_id)
        .await
        .map_err(|_| ErrorResponse::new(ErrorCode::AccessDenied))?;
    state
        .control_plane
        .validate_storage_resource(
            token,
            &owner,
            &state.storage_audience,
            body.application_id,
            body.kind.resource_kind(),
            &body.id.to_string(),
        )
        .await
        .map_err(|_| ErrorResponse::new(ErrorCode::AccessDenied))?;

    let previous = state
        .selection_policies
        .get(device_id, &owner, body.application_id)
        .await
        .map_err(ErrorResponse::from)?;
    if previous
        .as_ref()
        .is_some_and(|policy| !policy.admin_allowed)
    {
        return Err(ErrorResponse::new(ErrorCode::AccessDenied));
    }
    state
        .selection_policies
        .set_prevalidated(SelectionPolicy {
            device_id,
            owner_subject: owner,
            application_id: Some(body.application_id),
            selected_kind: Some(body.kind.policy_kind().to_owned()),
            selected_id: Some(body.id),
            admin_allowed: previous.is_none_or(|policy| policy.admin_allowed),
        })
        .await
        .map_err(ErrorResponse::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/devices/{device_id}/selection",
    params(("device_id" = String, Path, description = "Device ID")),
    responses((status = 204, description = "Device selection removed")),
    security(("authorization" = []))
)]
pub(crate) async fn delete_device_selection(
    State(state): State<RouterState>,
    Path(device_id): Path<Id>,
    authorization: ManagementAuthorization,
) -> Result<StatusCode, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        SELECTION_PERMISSION,
    )
    .await?;
    let owner = authorization.principal.get_entity_id().to_string();
    if !state
        .selection_policies
        .deselect_owned(device_id, &owner)
        .await
        .map_err(ErrorResponse::from)?
    {
        return Err(ErrorResponse::new(ErrorCode::NotFound));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/devices/{device_id}/selection/{application_id}/{kind}/{resource_id}",
    params(
        ("device_id" = String, Path, description = "Device ID"),
        ("application_id" = String, Path, description = "Application ID"),
        ("kind" = String, Path, description = "Resource kind"),
        ("resource_id" = String, Path, description = "Resource ID")
    ),
    responses((status = 204, description = "Device resource selection removed")),
    security(("authorization" = []))
)]
pub(crate) async fn delete_device_resource_selection(
    State(state): State<RouterState>,
    Path((device_id, application_id, kind, resource_id)): Path<(Id, Id, SelectionKind, Id)>,
    authorization: ManagementAuthorization,
) -> Result<StatusCode, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        SELECTION_PERMISSION,
    )
    .await?;
    let owner = authorization.principal.get_entity_id().to_string();
    if !state
        .selection_policies
        .deselect_resource_owned(
            device_id,
            &owner,
            application_id,
            kind.policy_kind(),
            resource_id,
        )
        .await
        .map_err(ErrorResponse::from)?
    {
        return Err(ErrorResponse::new(ErrorCode::NotFound));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RestrictionRequest {
    admin_allowed: bool,
}

#[utoipa::path(
    put,
    path = "/devices/{device_id}/restriction",
    params(("device_id" = String, Path, description = "Device ID")),
    request_body = RestrictionRequest,
    responses((status = 204, description = "Device restriction updated")),
    security(("authorization" = []))
)]
pub(crate) async fn put_device_restriction(
    State(state): State<RouterState>,
    Path(device_id): Path<Id>,
    authorization: ManagementAuthorization,
    Json(body): Json<RestrictionRequest>,
) -> Result<StatusCode, ErrorResponse> {
    require_application_permission(
        state.management_service.as_ref(),
        &authorization,
        RESTRICT_PERMISSION,
    )
    .await?;
    let devices = state
        .devices
        .as_ref()
        .ok_or_else(|| ErrorResponse::new(ErrorCode::AccessDenied))?;
    let device = devices
        .list()
        .await
        .map_err(|_| ErrorResponse::new(ErrorCode::AccessDenied))?
        .into_iter()
        .find(|device| device.id == device_id && device.state == DeviceState::Approved)
        .ok_or_else(|| ErrorResponse::new(ErrorCode::NotFound))?;
    state
        .selection_policies
        .set_admin_allowed_prevalidated(device_id, &device.owner_subject, body.admin_allowed)
        .await
        .map_err(ErrorResponse::from)?
        .then_some(StatusCode::NO_CONTENT)
        .ok_or_else(|| ErrorResponse::new(ErrorCode::AccessDenied))
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};
    use idp_model::contract::ErrorCode;

    use super::{SelectionKind, SelectionRequest, storage_token};

    #[test]
    fn selection_request_accepts_only_supported_kinds() {
        let id = "00000000-0000-0000-0000-000000000001";
        for kind in ["database", "filesystem"] {
            let body = format!(r#"{{"applicationId":"{id}","kind":"{kind}","id":"{id}"}}"#);
            let parsed: SelectionRequest = serde_json::from_str(&body).expect("supported kind");
            assert!(matches!(
                parsed.kind,
                SelectionKind::Database | SelectionKind::Filesystem
            ));
        }
        let body = format!(r#"{{"applicationId":"{id}","kind":"unknown","id":"{id}"}}"#);
        assert!(serde_json::from_str::<SelectionRequest>(&body).is_err());
    }

    #[test]
    fn storage_header_requires_separate_bearer_token() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            storage_token(&headers).err().map(|error| error.error),
            Some(ErrorCode::NotAuthorized)
        );
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer management"),
        );
        assert!(storage_token(&headers).is_err());
        headers.insert(
            "x-storage-authorization",
            HeaderValue::from_static("Bearer storage"),
        );
        assert_eq!(storage_token(&headers).expect("storage token"), "storage");
        headers.insert(
            "x-storage-authorization",
            HeaderValue::from_static("Bearer "),
        );
        assert!(storage_token(&headers).is_err());
    }
}
