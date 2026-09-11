use axum::{Json, extract::State, http::StatusCode};
use model::contract::{HealthResponse, HealthStatus};

use crate::RouterState;

#[utoipa::path(
    get,
    path = "/health",
    responses((status = OK, description = "Server is healthy", body = HealthResponse))
)]
pub(crate) async fn health(
    State(state): State<RouterState>,
) -> Result<Json<HealthResponse>, (StatusCode, Json<HealthResponse>)> {
    let database = match state.database.connect() {
        Ok(_) => HealthStatus::Healthy,
        Err(e) => HealthStatus::Unhealthy(e.to_string()),
    };
    let health_response = HealthResponse { database };

    if health_response.is_healthy() {
        Ok(Json(health_response))
    } else {
        Err((StatusCode::SERVICE_UNAVAILABLE, Json(health_response)))
    }
}
