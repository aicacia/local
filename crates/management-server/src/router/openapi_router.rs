use api::SecurityAddon;
use utoipa::OpenApi;
use utoipa::openapi::{Paths, RefOr, Schema, Server};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::RouterState;

use super::openapi::{__path_openapi_json, openapi_json};
use super::routes::applications::{
    __path_create_application, __path_delete_application, __path_get_application,
    __path_list_applications, __path_update_application, create_application, delete_application,
    get_application, list_applications, update_application,
};
use super::routes::clients::{
    __path_create_client, __path_delete_client, __path_get_client, __path_list_clients,
    __path_update_client, create_client, delete_client, get_client, list_clients, update_client,
};
use super::routes::consents::{
    __path_list_user_consents, __path_revoke_user_consent, list_user_consents, revoke_user_consent,
};
use super::routes::health::{__path_health, health};
use super::routes::keys::{
    __path_get_key_jwk, __path_list_client_keys, get_key_jwk, list_client_keys,
};
use super::routes::permissions::{
    __path_assign_permission_to_role, __path_create_permission, __path_delete_permission,
    __path_list_permissions, __path_list_role_permissions, __path_revoke_permission_from_role,
    assign_permission_to_role, create_permission, delete_permission, list_permissions,
    list_role_permissions, revoke_permission_from_role,
};
use super::routes::roles::{
    __path_assign_role_to_user, __path_create_role, __path_delete_role, __path_list_roles,
    __path_list_user_roles, __path_revoke_role_from_user, assign_role_to_user, create_role,
    delete_role, list_roles, list_user_roles, revoke_role_from_user,
};
use super::routes::users::{
    __path_get_user, __path_list_user_roles_across_applications, __path_list_users, get_user,
    list_user_roles_across_applications, list_users,
};
use super::routes::version::{__path_version, version};

#[derive(OpenApi)]
#[openapi(
    info(title = "LIDP Management API", version = env!("CARGO_PKG_VERSION")),
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
            .routes(routes!(list_applications))
            .routes(routes!(create_application))
            .routes(routes!(get_application))
            .routes(routes!(update_application))
            .routes(routes!(delete_application))
            .routes(routes!(list_clients))
            .routes(routes!(create_client))
            .routes(routes!(get_client))
            .routes(routes!(update_client))
            .routes(routes!(delete_client))
            .routes(routes!(list_client_keys))
            .routes(routes!(get_key_jwk))
            .routes(routes!(list_users))
            .routes(routes!(get_user))
            .routes(routes!(list_user_roles_across_applications))
            .routes(routes!(list_user_consents))
            .routes(routes!(revoke_user_consent))
            .routes(routes!(list_roles))
            .routes(routes!(create_role))
            .routes(routes!(delete_role))
            .routes(routes!(list_user_roles))
            .routes(routes!(assign_role_to_user))
            .routes(routes!(revoke_role_from_user))
            .routes(routes!(list_permissions))
            .routes(routes!(create_permission))
            .routes(routes!(delete_permission))
            .routes(routes!(list_role_permissions))
            .routes(routes!(assign_permission_to_role))
            .routes(routes!(revoke_permission_from_role))
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
        openapi_json_path.to_string(),
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
