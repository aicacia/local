use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

#[derive(OpenApi)]
#[openapi(info(title = "OAuth Server", version = env!("CARGO_PKG_VERSION")))]
pub(crate) struct ApiDoc;

pub fn openapi_router(
    idp_router: OpenApiRouter,
    management_router: OpenApiRouter,
    storage_session_router: OpenApiRouter,
) -> OpenApiRouter {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(idp_router)
        .merge(management_router)
        .merge(storage_session_router)
}
