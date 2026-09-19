use axum::{Json, extract::State};
use model::contract::{HealthResponse, HealthStatus};

use crate::RouterState;

#[utoipa::path(
    get,
    path = "/health",
    responses((status = OK, description = "Server is healthy", body = HealthResponse))
)]
pub(crate) async fn health(State(_state): State<RouterState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        database: HealthStatus::Healthy,
    })
}
