use axum::{
    Json,
    extract::{Query, RawQuery, State},
    response::Redirect,
};
use idp_model::contract::{
    AuthorizationCodeResponse, AuthorizationRequest, ErrorCode, ErrorResponse,
};

use crate::router::{
    RouterState,
    middleware::{StandardAuthorization, require_current_global_identity},
};

#[utoipa::path(
    post,
    path = "/oauth2/auth",
    request_body(content = AuthorizationRequest, content_type = "application/json"),
    responses((status = 200, description = "Authorization response", body = AuthorizationCodeResponse)),
    security(
        ("authorization" = [])
    )
)]
pub(crate) async fn authorize_json(
    State(state): State<RouterState>,
    StandardAuthorization { principal, .. }: StandardAuthorization,
    Json(request): Json<AuthorizationRequest>,
) -> Result<Json<AuthorizationCodeResponse>, ErrorResponse> {
    let response = state
        .oauth2_service
        .authorize(request, principal.as_ref())
        .await?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/oauth2/auth",
    params(
        AuthorizationRequest
    ),
    responses((status = 302, description = "Authorization response"))
)]
pub(crate) async fn authorize_query(
    State(state): State<RouterState>,
    RawQuery(raw_query): RawQuery,
    Query(query): Query<AuthorizationRequest>,
) -> Result<Redirect, ErrorResponse> {
    redirect_to_authorize_ui(state, query, raw_query).await
}

async fn redirect_to_authorize_ui(
    state: RouterState,
    request: AuthorizationRequest,
    raw_query: Option<String>,
) -> Result<Redirect, ErrorResponse> {
    require_current_global_identity(&state).await?;
    let mut redirect_url = state.ui_base_uri.clone();
    let query_string = if let Some(raw_query) = raw_query {
        raw_query
    } else {
        serde_qs::to_string(&request).map_err(|e| {
            ErrorResponse::new(ErrorCode::ServerError).with_description(e.to_string())
        })?
    };

    if !redirect_url.ends_with('/') {
        redirect_url.push('/');
    }
    redirect_url.push_str("authorize");
    redirect_url.push('?');
    redirect_url.push_str(&query_string);

    Ok(Redirect::to(&redirect_url))
}
