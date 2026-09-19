use axum::{Json, extract::State, http::StatusCode};
use model::contract::{HealthResponse, HealthStatus};

use crate::RouterState;

#[utoipa::path(
    get,
    path = "/health",
    responses((status = OK, description = "Server is healthy", body = HealthResponse))
)]
pub(crate) async fn health(
    State(_state): State<RouterState>,
) -> Result<Json<HealthResponse>, (StatusCode, Json<HealthResponse>)> {
    let health_response = HealthResponse {
        database: HealthStatus::Healthy,
    };

    if health_response.is_healthy() {
        Ok(Json(health_response))
    } else {
        Err((StatusCode::SERVICE_UNAVAILABLE, Json(health_response)))
    }
}
