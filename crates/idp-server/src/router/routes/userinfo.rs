use axum::{Json, extract::State};
use idp_model::contract::{EntityType, ErrorCode, ErrorResponse, UserInfo};

use crate::router::{RouterState, middleware::StandardAuthorization};

#[utoipa::path(
    get,
    path = "/userinfo",
    responses((status = 200, description = "Userinfo", body = UserInfo)),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn userinfo(
    State(state): State<RouterState>,
    StandardAuthorization { principal, .. }: StandardAuthorization,
) -> Result<Json<UserInfo>, ErrorResponse> {
    if principal.get_entity_type() != EntityType::User {
        return Err(ErrorResponse::new(ErrorCode::AccessDenied)
            .with_description("Only users can access this endpoint".to_string()));
    }

    let user_info = state
        .oauth2_service
        .find_user_info(principal.get_entity_id())
        .await?;

    Ok(Json(user_info))
}
