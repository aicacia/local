use axum::{Json, extract::State};
use utoipa::openapi::OpenApi as OpenApiDocument;

#[utoipa::path(get, path = "/openapi.json", responses((status = 200, description = "OpenAPI specification")))]
pub(crate) async fn openapi_json(State(openapi): State<OpenApiDocument>) -> Json<OpenApiDocument> {
    Json(openapi)
}
