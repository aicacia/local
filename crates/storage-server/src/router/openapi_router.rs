use api::SecurityAddon;
use utoipa::OpenApi;
use utoipa::openapi::{Paths, RefOr, Schema, Server};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::RouterState;

use super::openapi::{__path_openapi_json, openapi_json};
use super::routes::health::{__path_health, health};
use super::routes::version::{__path_version, version};

#[derive(OpenApi)]
#[openapi(
    info(title = "OAuth Server", version = env!("CARGO_PKG_VERSION")),
    modifiers(&SecurityAddon)
)]
pub(crate) struct ApiDoc;

pub fn openapi_router(router_state: RouterState, prefix: &str) -> OpenApiRouter {
    let prefix = if prefix == "/" { "" } else { prefix };
    let api_base_uri = router_state.api_base_uri.clone();

    let routes = || {
        OpenApiRouter::new()
            .routes(routes!(health))
            .routes(routes!(version))
    };

    let spec_router = OpenApiRouter::with_openapi(ApiDoc::openapi()).merge(routes());

    let mut openapi_spec = spec_router.get_openapi().clone();

    openapi_spec.servers = Some(vec![Server::new(format!("{}{}", api_base_uri, prefix))]);

    if !prefix.is_empty() {
        let mut paths = Paths::new();

        for (path, item) in openapi_spec.paths.paths {
            let path = path.strip_prefix(prefix).unwrap_or(&path).to_owned();

            paths.paths.insert(path, item);
        }

        openapi_spec.paths = paths;
    }

    let mut schemas = Vec::<(String, RefOr<Schema>)>::new();
    let (openapi_json_path, openapi_json_item, openapi_json_types) =
        routes!(@resolve_types openapi_json : schemas);

    openapi_spec.paths.add_path_operation(
        &openapi_json_path,
        openapi_json_types,
        openapi_json_item,
    );

    let runtime_router = if prefix.is_empty() {
        OpenApiRouter::with_openapi(ApiDoc::openapi()).merge(routes().with_state(router_state))
    } else {
        OpenApiRouter::with_openapi(ApiDoc::openapi())
            .nest(prefix, routes().with_state(router_state))
    };

    let openapi_json_routes = OpenApiRouter::new()
        .routes(routes!(openapi_json))
        .with_state(openapi_spec);

    if prefix.is_empty() {
        runtime_router.merge(openapi_json_routes)
    } else {
        runtime_router.nest(prefix, openapi_json_routes)
    }
}
