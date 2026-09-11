use axum::extract::{Form, State};
use idp_model::contract::{ErrorResponse, RevocationRequest};

use crate::router::RouterState;

#[utoipa::path(post, path = "/oauth2/revoke", request_body(content = RevocationRequest, content_type = "application/x-www-form-urlencoded"), responses((status = 200, description = "Revoke token")))]
pub(crate) async fn revoke(
    State(state): State<RouterState>,
    Form(request): Form<RevocationRequest>,
) -> Result<(), ErrorResponse> {
    state.oauth2_service.revoke(request).await
}
