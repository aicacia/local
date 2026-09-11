use axum::Json;
use model::contract::VersionResponse;

#[utoipa::path(get, path = "/version", responses((status = 200, description = "Version", body = VersionResponse)))]
pub(crate) async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        name: env!("CARGO_PKG_NAME").to_string(),
        ..VersionResponse::default()
    })
}
