use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use iroh::EndpointId;
use model::contract::{AuthorizationDetail, StandardClaims, StorageAuthorizationAction};
use serde::{Deserialize, Serialize};
use storage_server::{
    StorageSocketAccess, StorageSocketAuthorizer, storage_router as socket_router,
};
use storage_service::{
    DatabaseResource, FileSystemId, FileSystemResource, ScopedFileSystemRuntime,
    ScopedStorageService,
};

use crate::RouterState;

pub fn storage_router(
    state: RouterState,
    file_systems: Arc<ScopedFileSystemRuntime<EndpointId>>,
) -> Router {
    let file_system_routes = Router::new()
        .route(
            "/storage/filesystems",
            post(create_file_system).get(list_file_systems),
        )
        .route(
            "/storage/filesystems/{filesystem_id}",
            get(get_file_system).delete(delete_file_system),
        )
        .with_state(state.clone());
    let database_routes = Router::new()
        .route(
            "/storage/databases",
            post(create_database).get(list_databases),
        )
        .route(
            "/storage/databases/{database_id}",
            get(get_database).delete(delete_database),
        )
        .with_state(state.clone());
    socket_router(Arc::new(ScopedFileSystemSocketAuthorizer {
        state,
        file_systems,
    }))
    .merge(database_routes)
    .merge(file_system_routes)
}

struct ScopedFileSystemSocketAuthorizer {
    state: RouterState,
    file_systems: Arc<ScopedFileSystemRuntime<EndpointId>>,
}

impl StorageSocketAuthorizer for ScopedFileSystemSocketAuthorizer {
    type Session = ScopedStorageService<EndpointId, StorageNamespace>;

    async fn authorize(
        &self,
        token: String,
        resource_id: String,
    ) -> Result<StorageSocketAccess<Self::Session>, ()> {
        let resource_id = FileSystemId::parse(&resource_id).map_err(|_| ())?;
        let authorization = super::middleware::authorize_bearer(&self.state, &token)
            .await
            .map_err(|_| ())?;
        let (read, write) = storage_access(&authorization.claims).ok_or(())?;
        let client_id = &authorization.claims.client_id;
        let application_id = self
            .state
            .oauth2_service
            .application_id_for_client(client_id)
            .await
            .map_err(|_| ())?;
        let scope = StorageNamespace {
            user_sub: authorization.claims.sub,
            application_id,
        };
        let session = ScopedStorageService::open_resource(&self.file_systems, scope, resource_id)
            .await
            .map_err(|_| ())?;
        Ok(StorageSocketAccess {
            read,
            write,
            session,
        })
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct CreateDatabaseRequest {
    name: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct FileSystemResourceResponse {
    id: String,
    name: Option<String>,
}

impl From<FileSystemResource> for FileSystemResourceResponse {
    fn from(resource: FileSystemResource) -> Self {
        Self {
            id: resource.id.as_uuid().to_string(),
            name: resource.name,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct DatabaseDetailResponse {
    #[schema(value_type = String)]
    id: storage_service::DatabaseId,
    name: Option<String>,
    application_id: String,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct FileSystemDetailResponse {
    id: String,
    name: Option<String>,
    application_id: String,
}

#[utoipa::path(
    post,
    path = "/storage/databases",
    request_body = CreateDatabaseRequest,
    responses(
        (status = 201, description = "Database resource created", body = DatabaseResource),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit writes"),
        (status = 503, description = "Database runtime unavailable")
    ),
    security(("authorization" = []))
)]
pub(super) async fn create_database(
    State(state): State<RouterState>,
    authorization: super::middleware::StandardAuthorization,
    Json(request): Json<CreateDatabaseRequest>,
) -> Result<(StatusCode, Json<DatabaseResource>), StatusCode> {
    let (scope, _, can_write) = database_scope(&state, &authorization.claims).await?;
    if !can_write {
        return Err(StatusCode::FORBIDDEN);
    }
    let runtime = state
        .storage_databases
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let (resource, _) = runtime
        .create(&scope, request.name)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(resource)))
}

#[utoipa::path(
    get,
    path = "/storage/databases",
    responses(
        (status = 200, description = "Namespace database resources", body = [DatabaseResource]),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit reads"),
        (status = 503, description = "Database runtime unavailable")
    ),
    security(("authorization" = []))
)]
pub(super) async fn list_databases(
    State(state): State<RouterState>,
    authorization: super::middleware::StandardAuthorization,
) -> Result<Json<Vec<DatabaseResource>>, StatusCode> {
    let (scope, can_read, _) = database_scope(&state, &authorization.claims).await?;
    if !can_read {
        return Err(StatusCode::FORBIDDEN);
    }
    let runtime = state
        .storage_databases
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    runtime
        .list(&scope)
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[utoipa::path(
    get,
    path = "/storage/databases/{database_id}",
    params(("database_id" = String, Path, description = "Database resource ID")),
    responses(
        (status = 200, description = "Database resource", body = DatabaseDetailResponse),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit reads"),
        (status = 404, description = "Database resource not found"),
        (status = 503, description = "Database runtime unavailable")
    ),
    security(("authorization" = []))
)]
pub(super) async fn get_database(
    State(state): State<RouterState>,
    authorization: super::middleware::StandardAuthorization,
    Path(database_id): Path<storage_service::DatabaseId>,
) -> Result<Json<DatabaseDetailResponse>, StatusCode> {
    let (scope, can_read, _) = database_scope(&state, &authorization.claims).await?;
    if !can_read {
        return Err(StatusCode::FORBIDDEN);
    }
    state
        .storage_databases
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .get(&scope, database_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(|resource| {
            Json(DatabaseDetailResponse {
                id: resource.id,
                name: resource.name,
                application_id: scope.application_id.to_string(),
            })
        })
        .ok_or(StatusCode::NOT_FOUND)
}

#[utoipa::path(
    delete,
    path = "/storage/databases/{database_id}",
    params(("database_id" = String, Path, description = "Database resource ID")),
    responses(
        (status = 204, description = "Database resource tombstoned"),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit writes"),
        (status = 404, description = "Database resource not found"),
        (status = 503, description = "Database runtime unavailable")
    ),
    security(("authorization" = []))
)]
pub(super) async fn delete_database(
    State(state): State<RouterState>,
    authorization: super::middleware::StandardAuthorization,
    Path(database_id): Path<storage_service::DatabaseId>,
) -> Result<StatusCode, StatusCode> {
    let (scope, _, can_write) = database_scope(&state, &authorization.claims).await?;
    if !can_write {
        return Err(StatusCode::FORBIDDEN);
    }
    let deleted = state
        .storage_databases
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .delete(&scope, database_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[utoipa::path(
    post,
    path = "/storage/filesystems",
    request_body = CreateDatabaseRequest,
    responses(
        (status = 201, description = "Filesystem resource created", body = FileSystemResourceResponse),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit writes")
    ),
    security(("authorization" = []))
)]
pub(super) async fn create_file_system(
    State(state): State<RouterState>,
    authorization: super::middleware::StandardAuthorization,
    Json(request): Json<CreateDatabaseRequest>,
) -> Result<(StatusCode, Json<FileSystemResourceResponse>), StatusCode> {
    let (scope, _, can_write) = database_scope(&state, &authorization.claims).await?;
    if !can_write {
        return Err(StatusCode::FORBIDDEN);
    }
    let resource = state
        .storage_file_systems
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .create_resource(&scope, request.name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(resource.into())))
}

#[utoipa::path(
    get,
    path = "/storage/filesystems",
    responses(
        (status = 200, description = "Namespace filesystem resources", body = [FileSystemResourceResponse]),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit reads")
    ),
    security(("authorization" = []))
)]
pub(super) async fn list_file_systems(
    State(state): State<RouterState>,
    authorization: super::middleware::StandardAuthorization,
) -> Result<Json<Vec<FileSystemResourceResponse>>, StatusCode> {
    let (scope, can_read, _) = database_scope(&state, &authorization.claims).await?;
    if !can_read {
        return Err(StatusCode::FORBIDDEN);
    }
    state
        .storage_file_systems
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .list_resources(&scope)
        .await
        .map(|resources| Json(resources.into_iter().map(Into::into).collect()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[utoipa::path(
    get,
    path = "/storage/filesystems/{filesystem_id}",
    params(("filesystem_id" = String, Path, description = "Filesystem resource ID")),
    responses(
        (status = 200, description = "Filesystem resource", body = FileSystemDetailResponse),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit reads"),
        (status = 404, description = "Filesystem resource not found")
    ),
    security(("authorization" = []))
)]
pub(super) async fn get_file_system(
    State(state): State<RouterState>,
    authorization: super::middleware::StandardAuthorization,
    Path(resource_id): Path<String>,
) -> Result<Json<FileSystemDetailResponse>, StatusCode> {
    let (scope, can_read, _) = database_scope(&state, &authorization.claims).await?;
    if !can_read {
        return Err(StatusCode::FORBIDDEN);
    }
    let resource_id = FileSystemId::parse(&resource_id).map_err(|_| StatusCode::NOT_FOUND)?;
    state
        .storage_file_systems
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .list_resources(&scope)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .find(|resource| resource.id == resource_id)
        .map(|resource| {
            Json(FileSystemDetailResponse {
                id: resource.id.as_uuid().to_string(),
                name: resource.name,
                application_id: scope.application_id.to_string(),
            })
        })
        .ok_or(StatusCode::NOT_FOUND)
}

#[utoipa::path(
    delete,
    path = "/storage/filesystems/{filesystem_id}",
    params(("filesystem_id" = String, Path, description = "Filesystem resource ID")),
    responses(
        (status = 204, description = "Filesystem resource tombstoned"),
        (status = 401, description = "Invalid storage authorization"),
        (status = 403, description = "Token does not permit writes"),
        (status = 404, description = "Filesystem resource not found")
    ),
    security(("authorization" = []))
)]
pub(super) async fn delete_file_system(
    State(state): State<RouterState>,
    authorization: super::middleware::StandardAuthorization,
    Path(resource_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let (scope, _, can_write) = database_scope(&state, &authorization.claims).await?;
    if !can_write {
        return Err(StatusCode::FORBIDDEN);
    }
    let resource_id = FileSystemId::parse(&resource_id).map_err(|_| StatusCode::NOT_FOUND)?;
    let deleted = state
        .storage_file_systems
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .delete_resource(&scope, resource_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn database_scope(
    state: &RouterState,
    claims: &StandardClaims,
) -> Result<(StorageNamespace, bool, bool), StatusCode> {
    let (can_read, can_write) = storage_access(claims).ok_or(StatusCode::UNAUTHORIZED)?;
    let application_id = state
        .oauth2_service
        .application_id_for_client(&claims.client_id)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok((
        storage_namespace(claims, application_id),
        can_read,
        can_write,
    ))
}

fn storage_namespace(
    claims: &StandardClaims,
    application_id: idp_model::model::Id,
) -> StorageNamespace {
    StorageNamespace {
        user_sub: claims.sub.clone(),
        application_id,
    }
}

fn storage_access(claims: &StandardClaims) -> Option<(bool, bool)> {
    let resource = claims.resource.as_deref()?;
    if claims.aud != resource {
        return None;
    }
    let [AuthorizationDetail::Storage(detail)] = claims.authorization_details.as_deref()? else {
        return None;
    };
    Some((
        detail.actions.contains(&StorageAuthorizationAction::Read),
        detail.actions.contains(&StorageAuthorizationAction::Write),
    ))
}

#[derive(Debug, Eq, PartialEq)]
struct StorageNamespace {
    user_sub: String,
    application_id: idp_model::model::Id,
}

impl storage_model::StorageNamespace for StorageNamespace {
    fn user_sub(&self) -> &str {
        &self.user_sub
    }

    fn application_id(&self) -> idp_model::model::Id {
        self.application_id
    }
}

#[cfg(test)]
mod tests {
    use std::{
        any::Any,
        collections::HashMap,
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{body::to_bytes, http::Request};
    use idp_model::{
        contract::{
            ApplicationRegistration, ClientProfile, ClientRegistration, ClientType, EntityType,
            GrantType, ResponseType, TokenEndpointAuthMethod,
        },
        replica::up,
    };
    use idp_service::{
        PasswordConfig,
        oauth2::{OAuth2Config, OAuth2Service, encode_jwt},
        replica::{
            DbApplicationRepo, DbClientRepo, DbKeyRepo, DbOAuth2AuthorizationCodeRepo,
            DbOAuth2UserConsentRepo, DbUserRepo,
        },
        repo::{ClientRepo, KeyRepo, KeyService, PrivateKeyKeyringRepo, PrivateKeyRepo, UserRepo},
    };
    use iroh::{Endpoint, SecretKey, endpoint::presets};
    use keyring_core::api::{CredentialPersistence, CredentialStoreApi};
    use management_server::{
        RouterState as ManagementRouterState, openapi_router as management_router,
    };
    use management_service::{
        DeviceRepo, HostedControlPlane, MANAGEMENT_APPLICATION_URI, ManagementService,
        replica::{DbPermissionRepo, DbRoleRepo, DbSelectionPolicyRepo},
    };
    use model::contract::{StorageAuthorizationDetail, TokenType, TokenUse};
    use tower::ServiceExt;

    use crate::{
        DeviceIdentity,
        router::state::{NativeDeviceRepo, NativeOAuth2ServiceRef, RouterState},
    };

    use super::*;

    struct ModifierTolerantMockStore(Arc<keyring_core::mock::Store>);

    impl CredentialStoreApi for ModifierTolerantMockStore {
        fn vendor(&self) -> String {
            self.0.vendor()
        }

        fn id(&self) -> String {
            self.0.id()
        }

        fn build(
            &self,
            service: &str,
            user: &str,
            _modifiers: Option<&HashMap<&str, &str>>,
        ) -> keyring_core::Result<keyring_core::Entry> {
            self.0.build(service, user, None)
        }

        fn search(
            &self,
            spec: &HashMap<&str, &str>,
        ) -> keyring_core::Result<Vec<keyring_core::Entry>> {
            self.0.search(spec)
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn persistence(&self) -> CredentialPersistence {
            CredentialPersistence::ProcessOnly
        }
    }

    struct Fixture {
        router: Router,
        oauth2: Arc<NativeOAuth2ServiceRef>,
        key_service: Arc<
            KeyService<DbKeyRepo<ofdb::RedbKernel, ofdb::AutomergeRowCodec>, PrivateKeyKeyringRepo>,
        >,
        user_id: idp_model::model::Id,
        second_user_id: idp_model::model::Id,
        devices: Arc<NativeDeviceRepo>,
        selection_policies: Arc<DbSelectionPolicyRepo<ofdb::RedbKernel, ofdb::AutomergeRowCodec>>,
        management: Arc<
            ManagementService<
                DbApplicationRepo<ofdb::RedbKernel, ofdb::AutomergeRowCodec>,
                DbPermissionRepo<ofdb::RedbKernel, ofdb::AutomergeRowCodec>,
                DbRoleRepo<ofdb::RedbKernel, ofdb::AutomergeRowCodec>,
            >,
        >,
        temp_dir: PathBuf,
        _endpoint: Endpoint,
        _server: Option<tokio::task::JoinHandle<()>>,
    }

    impl Fixture {
        async fn new() -> Self {
            Self::new_with_management(false).await
        }

        async fn new_with_management(mount_management: bool) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos();
            let temp_dir = std::env::temp_dir().join(format!(
                "idp-storage-routes-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&temp_dir).expect("create fixture directory");
            let engine = Arc::new(
                db::open_native_engine(temp_dir.join("idp.redb"))
                    .expect("open temporary Redb database"),
            );
            up(&engine).await.expect("apply IdP migrations");

            let store = ModifierTolerantMockStore(
                keyring_core::mock::Store::new().expect("create in-memory keyring mock"),
            );
            let store: Arc<keyring_core::CredentialStore> = Arc::new(store);
            let key_service = Arc::new(KeyService::new(
                DbKeyRepo::new(Arc::clone(&engine)),
                PrivateKeyKeyringRepo::new_with_store("idp-storage-tests", store),
                "idp-storage-tests",
            ));
            let client_repo = DbClientRepo::new(Arc::clone(&engine), Arc::clone(&key_service));
            let user_repo = DbUserRepo::new(Arc::clone(&engine), PasswordConfig::default());
            let user = user_repo
                .create_user_with_password("Storage test user", "test-password")
                .await
                .expect("create persisted test user");
            key_service
                .ensure_entity_master_key(EntityType::User, user.id, "")
                .expect("create persisted user master key");
            key_service
                .rotate_active_entity_root_key(
                    EntityType::User,
                    user.id,
                    "Storage test user".into(),
                    None,
                )
                .await
                .expect("create persisted user signing key");
            let second_user = user_repo
                .create_user_with_password("Other storage user", "test-password")
                .await
                .expect("create second persisted test user");
            key_service
                .ensure_entity_master_key(EntityType::User, second_user.id, "")
                .expect("create persisted second user master key");
            key_service
                .rotate_active_entity_root_key(
                    EntityType::User,
                    second_user.id,
                    "Other storage user".into(),
                    None,
                )
                .await
                .expect("create second persisted user signing key");

            let client_registration = |client_id: &str, app_uri: &str| ClientRegistration {
                application: ApplicationRegistration {
                    name: Some(client_id.into()),
                    uri: app_uri.into(),
                    description: None,
                },
                client_id: Some(client_id.into()),
                client_secret: None,
                client_id_issued_at: None,
                client_secret_expires_at: None,
                client_name: client_id.into(),
                client_uri: None,
                logo_uri: None,
                contacts: Vec::new(),
                terms_of_service_uri: None,
                policy_uri: None,
                client_type: ClientType::Public,
                profile: ClientProfile::Web,
                redirect_uris: Vec::new(),
                allowed_grant_types: vec![GrantType::AuthorizationCode],
                response_types: vec![ResponseType::Code],
                allowed_scopes: vec!["openid".into()],
                token_endpoint_auth_method: TokenEndpointAuthMethod::None,
                software_statement: None,
                software_id: None,
                software_version: None,
            };
            client_repo
                .create_client(client_registration(
                    "storage-client",
                    "https://storage.example",
                ))
                .await
                .expect("persist client and application mapping");
            client_repo
                .create_client(client_registration(
                    "same-application-client",
                    "https://storage.example",
                ))
                .await
                .expect("persist second client for the same application");
            client_repo
                .create_client(client_registration("other-client", "https://other.example"))
                .await
                .expect("persist second application mapping");

            let oauth2 = Arc::new(OAuth2Service::new(
                DbApplicationRepo::new(Arc::clone(&engine)),
                client_repo,
                DbOAuth2AuthorizationCodeRepo::new(Arc::clone(&engine)),
                user_repo,
                DbOAuth2UserConsentRepo::new(Arc::clone(&engine)),
                Arc::clone(&key_service),
                OAuth2Config {
                    issuer: "https://issuer.example".into(),
                    ..OAuth2Config::default()
                },
            ));

            let secret_key = SecretKey::generate();
            let endpoint = Endpoint::builder(presets::N0)
                .secret_key(secret_key.clone())
                .bind()
                .await
                .expect("bind fixture Iroh endpoint");
            let identity = Arc::new(DeviceIdentity::new(endpoint.clone(), secret_key));
            let file_systems = Arc::new(
                ScopedFileSystemRuntime::new(temp_dir.join("filesystems"), endpoint.id())
                    .expect("create temporary filesystem runtime"),
            );
            let devices = Arc::new(NativeDeviceRepo::new(Arc::clone(&engine)));
            let management = Arc::new(ManagementService::new(
                DbApplicationRepo::new(Arc::clone(&engine)),
                DbPermissionRepo::new(Arc::clone(&engine)),
                DbRoleRepo::new(Arc::clone(&engine)),
            ));
            let selection_policies = Arc::new(DbSelectionPolicyRepo::new(Arc::clone(&engine)));
            let listener = if mount_management {
                Some(
                    tokio::net::TcpListener::bind("127.0.0.1:0")
                        .await
                        .expect("bind fixture IdP HTTP listener"),
                )
            } else {
                None
            };
            let idp_url = listener.as_ref().map(|listener| {
                format!("http://{}", listener.local_addr().expect("fixture address"))
            });
            let management_state = mount_management.then(|| {
                ManagementRouterState::new(
                    "https://issuer.example",
                    Arc::clone(&management),
                    Arc::clone(&oauth2),
                    Arc::clone(&selection_policies),
                    Arc::new(
                        HostedControlPlane::new_with_issuer(
                            idp_url.as_deref().expect("mounted IdP listener"),
                            "https://issuer.example",
                        )
                        .expect("create management control plane"),
                    ),
                    "storage",
                )
                .with_devices(Arc::clone(&devices))
            });
            let state = RouterState::new(
                "https://issuer.example",
                "https://issuer.example",
                Arc::clone(&engine),
                Arc::clone(&oauth2),
                Arc::clone(&devices),
                identity,
            )
            .with_storage_databases(Arc::new(
                storage_service::DatabaseRuntime::new(temp_dir.join("databases"))
                    .expect("create temporary database runtime"),
            ))
            .with_storage_file_systems(Arc::clone(&file_systems));

            let mut router = storage_router(state.clone(), file_systems);
            if mount_management {
                router = router.merge(
                    Router::new()
                        .route(
                            "/.well-known/jwks.json",
                            get(crate::router::routes::well_known::jwks),
                        )
                        .route(
                            "/devices",
                            get(crate::router::routes::devices::list_devices),
                        )
                        .with_state(state),
                );
            }
            if let Some(management_state) = management_state {
                router = router.merge(
                    management_router(management_state, "/management")
                        .split_for_parts()
                        .0,
                );
            }
            let server = listener.map(|listener| {
                let served_router = router.clone();
                tokio::spawn(async move {
                    axum::serve(listener, served_router)
                        .await
                        .expect("serve fixture IdP router");
                })
            });
            Self {
                router,
                oauth2,
                key_service,
                user_id: user.id,
                second_user_id: second_user.id,
                devices,
                selection_policies,
                management,
                temp_dir,
                _endpoint: endpoint,
                _server: server,
            }
        }

        async fn token(
            &self,
            client_id: &str,
            subject: idp_model::model::Id,
            actions: Vec<StorageAuthorizationAction>,
        ) -> String {
            let key = self
                .key_service
                .key_repo()
                .find_active_entity_root_key(EntityType::User, subject)
                .await
                .expect("find persisted user root key")
                .expect("user root key exists");
            let namespace = self.key_service.scoped_namespace(EntityType::User, subject);
            let private_key = self
                .key_service
                .private_key_repo()
                .load(&namespace, &key.derivation_path().expect("valid key path"))
                .expect("load persisted private key")
                .expect("private key exists");
            let jwk = key
                .to_jwk_private(&private_key)
                .expect("derive signed token JWK");
            let now = std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_secs() as i64;
            let claims = StandardClaims {
                r#type: TokenType::Bearer,
                r#use: TokenUse::Access,
                exp: now + 600,
                iat: now,
                nbf: now,
                iss: self.oauth2.metadata().issuer,
                aud: "storage".into(),
                client_id: client_id.into(),
                sub: subject.to_string(),
                resource: Some("storage".into()),
                authorization_details: Some(vec![AuthorizationDetail::Storage(
                    StorageAuthorizationDetail { actions },
                )]),
                scope: vec!["storage".into()],
            };
            encode_jwt(&jwk, &claims).expect("sign real IdP JWT")
        }

        async fn management_token(&self) -> String {
            let key = self
                .key_service
                .key_repo()
                .find_active_entity_root_key(EntityType::User, self.user_id)
                .await
                .expect("find persisted user root key")
                .expect("user root key exists");
            let namespace = self
                .key_service
                .scoped_namespace(EntityType::User, self.user_id);
            let private_key = self
                .key_service
                .private_key_repo()
                .load(&namespace, &key.derivation_path().expect("valid key path"))
                .expect("load persisted private key")
                .expect("private key exists");
            let jwk = key
                .to_jwk_private(&private_key)
                .expect("derive signed token JWK");
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_secs() as i64;
            encode_jwt(
                &jwk,
                &StandardClaims {
                    r#type: TokenType::Bearer,
                    r#use: TokenUse::Access,
                    exp: now + 600,
                    iat: now,
                    nbf: now,
                    iss: self.oauth2.metadata().issuer,
                    aud: MANAGEMENT_APPLICATION_URI.into(),
                    client_id: "storage-client".into(),
                    sub: self.user_id.to_string(),
                    resource: Some(MANAGEMENT_APPLICATION_URI.into()),
                    authorization_details: None,
                    scope: Vec::new(),
                },
            )
            .expect("sign real management JWT")
        }

        async fn selection_request(
            &self,
            method: http::Method,
            path: &str,
            management_token: &str,
            storage_token: Option<&str>,
            body: Option<&str>,
        ) -> axum::response::Response {
            let mut request = Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {management_token}"));
            if let Some(token) = storage_token {
                request = request.header("x-storage-authorization", format!("Bearer {token}"));
            }
            let body = body.map_or_else(axum::body::Body::empty, |body| {
                axum::body::Body::from(body.to_owned())
            });
            let request = request
                .header("content-type", "application/json")
                .body(body)
                .expect("build selection request");
            self.router
                .clone()
                .oneshot(request)
                .await
                .expect("route selection")
        }

        async fn request(
            &self,
            method: http::Method,
            path: &str,
            token: &str,
            body: Option<&str>,
        ) -> axum::response::Response {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {token}"));
            let request = match body {
                Some(body) => request
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_owned()))
                    .expect("build JSON request"),
                None => request
                    .body(axum::body::Body::empty())
                    .expect("build empty request"),
            };
            self.router
                .clone()
                .oneshot(request)
                .await
                .expect("route request")
        }
    }

    #[test]
    fn signed_management_restriction_requires_permission_and_approved_device() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("create restriction test runtime")
                    .block_on(check_signed_management_restriction());
            })
            .expect("spawn restriction test thread")
            .join()
            .expect("restriction test thread completed");
    }

    async fn check_signed_management_restriction() {
        let fixture = Fixture::new_with_management(true).await;
        let management_token = fixture.management_token().await;
        let storage_token = fixture
            .token(
                "storage-client",
                fixture.user_id,
                vec![StorageAuthorizationAction::Read],
            )
            .await;
        let approved = fixture
            .devices
            .create(
                fixture.user_id.to_string(),
                "approved".into(),
                "approved-key".into(),
                "approved-address".into(),
                vec![1],
                i64::MAX,
            )
            .await
            .expect("persist approved device");
        assert_eq!(approved.state, idp_model::contract::DeviceState::Approved);
        let pending = fixture
            .devices
            .create(
                fixture.user_id.to_string(),
                "pending".into(),
                "pending-key".into(),
                "pending-address".into(),
                vec![2],
                i64::MAX,
            )
            .await
            .expect("persist pending device");
        assert_eq!(pending.state, idp_model::contract::DeviceState::Pending);
        let path = format!("/management/devices/{}/restriction", approved.id);
        let restriction = Some(r#"{"adminAllowed":false}"#);
        let application = fixture
            .management
            .create_application("Management".into(), MANAGEMENT_APPLICATION_URI.into(), None)
            .await
            .expect("persist management application");

        let denied = fixture
            .request(http::Method::PUT, &path, &management_token, restriction)
            .await;
        let denied_status = denied.status();
        let denied_body = to_bytes(denied.into_body(), usize::MAX)
            .await
            .expect("read denied response");
        assert_eq!(
            denied_status,
            StatusCode::FORBIDDEN,
            "signed management token without permission must be denied: {}",
            String::from_utf8_lossy(&denied_body),
        );

        let role = fixture
            .management
            .create_role(application.id, "restrictor", None)
            .await
            .expect("persist restrictor role");
        let permission = fixture
            .management
            .create_permission(application.id, "devices.restrict", None)
            .await
            .expect("persist restriction permission");
        fixture
            .management
            .add_permission_to_role(application.id, role.id, permission.id)
            .await
            .expect("grant restriction permission to role");
        fixture
            .management
            .add_role_to_user(application.id, fixture.user_id, role.id)
            .await
            .expect("assign restrictor role to user");
        assert!(
            fixture
                .management
                .has_user_application_permission(fixture.user_id, "devices.restrict")
                .await
                .expect("check persisted permission")
        );

        assert_eq!(
            fixture
                .request(http::Method::PUT, &path, &storage_token, restriction)
                .await
                .status(),
            StatusCode::UNAUTHORIZED,
            "Storage-only signed token must not authorize Management",
        );
        for device_id in [idp_model::model::Id::now_v7(), pending.id] {
            assert_eq!(
                fixture
                    .request(
                        http::Method::PUT,
                        &format!("/management/devices/{device_id}/restriction"),
                        &management_token,
                        restriction,
                    )
                    .await
                    .status(),
                StatusCode::NOT_FOUND,
                "unknown or pending device must not be restricted",
            );
        }
        assert!(
            fixture
                .devices
                .revoke(&fixture.user_id.to_string(), pending.id, "protected-key")
                .await
                .expect("revoke pending device")
        );
        assert_eq!(
            fixture
                .request(
                    http::Method::PUT,
                    &format!("/management/devices/{}/restriction", pending.id),
                    &management_token,
                    restriction,
                )
                .await
                .status(),
            StatusCode::NOT_FOUND,
            "revoked device must not be restricted",
        );
        assert_eq!(
            fixture
                .request(http::Method::PUT, &path, &management_token, restriction)
                .await
                .status(),
            StatusCode::NO_CONTENT,
            "approved unselected device can be restricted",
        );
        let policy = fixture
            .selection_policies
            .get(approved.id, &fixture.user_id.to_string(), application.id)
            .await
            .expect("read persisted restriction")
            .expect("restriction exists");
        assert!(!policy.admin_allowed);
        assert_eq!(policy.application_id, None);
        assert_eq!(policy.selected_kind, None);
        assert_eq!(policy.selected_id, None);

        fixture._endpoint.close().await;
        std::fs::remove_dir_all(fixture.temp_dir).expect("remove fixture directory");
    }

    #[test]
    fn mounted_selection_validates_real_idp_storage_and_device_routes() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("create selection test runtime")
                    .block_on(check_mounted_selection());
            })
            .expect("spawn selection test thread")
            .join()
            .expect("selection test completed");
    }

    async fn check_mounted_selection() {
        let mut fixture = Fixture::new_with_management(true).await;
        let management_token = fixture.management_token().await;
        let storage_token = fixture
            .token(
                "storage-client",
                fixture.user_id,
                vec![StorageAuthorizationAction::Read],
            )
            .await;
        let write_token = fixture
            .token(
                "storage-client",
                fixture.user_id,
                vec![
                    StorageAuthorizationAction::Read,
                    StorageAuthorizationAction::Write,
                ],
            )
            .await;
        let application_id = fixture
            .oauth2
            .application_id_for_client("storage-client")
            .await
            .expect("resolve storage application");
        let management_app = fixture
            .management
            .create_application("Management".into(), MANAGEMENT_APPLICATION_URI.into(), None)
            .await
            .expect("persist Management application");
        let role = fixture
            .management
            .create_role(management_app.id, "selector", None)
            .await
            .expect("persist selection role");
        let permission = fixture
            .management
            .create_permission(management_app.id, "devices.select", None)
            .await
            .expect("persist selection permission");
        fixture
            .management
            .add_permission_to_role(management_app.id, role.id, permission.id)
            .await
            .expect("grant selection permission");
        fixture
            .management
            .add_role_to_user(management_app.id, fixture.user_id, role.id)
            .await
            .expect("assign selection role");
        let approved = fixture
            .devices
            .create(
                fixture.user_id.to_string(),
                "approved".into(),
                "selection-key".into(),
                "selection-address".into(),
                vec![1],
                i64::MAX,
            )
            .await
            .expect("persist approved device");
        assert_eq!(approved.state, idp_model::contract::DeviceState::Approved);
        let created = fixture
            .request(
                http::Method::POST,
                "/storage/databases",
                &write_token,
                Some(r#"{"name":"selected"}"#),
            )
            .await;
        assert_eq!(created.status(), StatusCode::CREATED);
        let resource: DatabaseResource = serde_json::from_slice(
            &to_bytes(created.into_body(), usize::MAX)
                .await
                .expect("read resource"),
        )
        .expect("decode resource");
        let path = format!("/management/devices/{}/selection", approved.id);
        let body = format!(
            r#"{{"applicationId":"{application_id}","kind":"database","id":"{}"}}"#,
            resource.id,
        );
        let select =
            |device_id: idp_model::model::Id| format!("/management/devices/{device_id}/selection");
        assert_eq!(
            fixture
                .selection_request(
                    http::Method::PUT,
                    &path,
                    &management_token,
                    Some(&storage_token),
                    Some(&body)
                )
                .await
                .status(),
            StatusCode::NO_CONTENT,
            "approved owner can select a real Storage resource",
        );
        let policy = fixture
            .selection_policies
            .get(approved.id, &fixture.user_id.to_string(), application_id)
            .await
            .expect("read selection")
            .expect("selection persisted");
        assert_eq!(policy.selected_id, Some(resource.id));
        assert_eq!(policy.selected_kind.as_deref(), Some("database"));

        let other_owner_token = fixture
            .token(
                "storage-client",
                fixture.second_user_id,
                vec![StorageAuthorizationAction::Read],
            )
            .await;
        assert_eq!(
            fixture
                .selection_request(
                    http::Method::PUT,
                    &path,
                    &management_token,
                    Some(&other_owner_token),
                    Some(&body)
                )
                .await
                .status(),
            StatusCode::FORBIDDEN,
            "another owner's Storage token cannot select",
        );
        let other_application = fixture
            .oauth2
            .application_id_for_client("other-client")
            .await
            .expect("resolve other application");
        let wrong_app = body.replace(&application_id.to_string(), &other_application.to_string());
        let wrong_kind = body.replace("database", "filesystem");
        for (label, attempted) in [("application", wrong_app), ("kind", wrong_kind)] {
            assert_eq!(
                fixture
                    .selection_request(
                        http::Method::PUT,
                        &path,
                        &management_token,
                        Some(&storage_token),
                        Some(&attempted)
                    )
                    .await
                    .status(),
                StatusCode::FORBIDDEN,
                "{label} substitution must fail",
            );
        }
        assert_eq!(
            fixture
                .selection_request(
                    http::Method::PUT,
                    &path,
                    &management_token,
                    None,
                    Some(&body)
                )
                .await
                .status(),
            StatusCode::UNAUTHORIZED,
            "selection requires a separate Storage token",
        );
        assert_eq!(
            fixture
                .selection_request(
                    http::Method::PUT,
                    &path,
                    &storage_token,
                    Some(&storage_token),
                    Some(&body)
                )
                .await
                .status(),
            StatusCode::UNAUTHORIZED,
            "Storage token cannot authorize Management",
        );
        let pending = fixture
            .devices
            .create(
                fixture.user_id.to_string(),
                "pending".into(),
                "pending-selection-key".into(),
                "pending-selection-address".into(),
                vec![2],
                i64::MAX,
            )
            .await
            .expect("persist pending device");
        assert_eq!(pending.state, idp_model::contract::DeviceState::Pending);
        for device_id in [pending.id, idp_model::model::Id::now_v7()] {
            assert_eq!(
                fixture
                    .selection_request(
                        http::Method::PUT,
                        &select(device_id),
                        &management_token,
                        Some(&storage_token),
                        Some(&body)
                    )
                    .await
                    .status(),
                StatusCode::FORBIDDEN,
                "pending or unknown device cannot be selected",
            );
        }
        assert!(
            fixture
                .devices
                .revoke(&fixture.user_id.to_string(), pending.id, "protected-key")
                .await
                .expect("revoke pending device")
        );
        assert_eq!(
            fixture
                .selection_request(
                    http::Method::PUT,
                    &select(pending.id),
                    &management_token,
                    Some(&storage_token),
                    Some(&body)
                )
                .await
                .status(),
            StatusCode::FORBIDDEN,
            "revoked device cannot be selected",
        );
        assert_eq!(
            fixture
                .request(
                    http::Method::DELETE,
                    &format!("/storage/databases/{}", resource.id),
                    &write_token,
                    None
                )
                .await
                .status(),
            StatusCode::NO_CONTENT,
        );
        assert_eq!(
            fixture
                .selection_request(
                    http::Method::PUT,
                    &path,
                    &management_token,
                    Some(&storage_token),
                    Some(&body)
                )
                .await
                .status(),
            StatusCode::FORBIDDEN,
            "deleted resource cannot be selected",
        );
        let live = fixture
            .request(
                http::Method::POST,
                "/storage/databases",
                &write_token,
                Some(r#"{"name":"live"}"#),
            )
            .await;
        assert_eq!(live.status(), StatusCode::CREATED);
        let live: DatabaseResource = serde_json::from_slice(
            &to_bytes(live.into_body(), usize::MAX)
                .await
                .expect("read live resource"),
        )
        .expect("decode live resource");
        let live_body = format!(
            r#"{{"applicationId":"{application_id}","kind":"database","id":"{}"}}"#,
            live.id,
        );
        let server = fixture._server.take().expect("mounted IdP server");
        server.abort();
        let _ = server.await;
        let policy = fixture
            .selection_policies
            .get(approved.id, &fixture.user_id.to_string(), application_id)
            .await
            .expect("read policy after denials")
            .expect("selection remains persisted");
        assert_eq!(policy.selected_id, Some(resource.id));
        assert_eq!(
            fixture
                .selection_request(
                    http::Method::PUT,
                    &path,
                    &management_token,
                    Some(&storage_token),
                    Some(&live_body),
                )
                .await
                .status(),
            StatusCode::FORBIDDEN,
            "Storage outage must block selection",
        );
        assert_eq!(
            fixture
                .selection_request(http::Method::DELETE, &path, &management_token, None, None)
                .await
                .status(),
            StatusCode::NO_CONTENT,
            "owner can deselect without Storage",
        );
        let policy = fixture
            .selection_policies
            .get(approved.id, &fixture.user_id.to_string(), application_id)
            .await
            .expect("read deselected policy")
            .expect("policy exists");
        assert_eq!(policy.selected_id, None);
        fixture._endpoint.close().await;
        std::fs::remove_dir_all(fixture.temp_dir).expect("remove fixture directory");
    }

    #[tokio::test]
    async fn authenticated_database_storage_crud_is_scoped_and_enforces_actions() {
        let fixture = Fixture::new().await;
        let write_token = fixture
            .token(
                "storage-client",
                fixture.user_id,
                vec![
                    StorageAuthorizationAction::Read,
                    StorageAuthorizationAction::Write,
                ],
            )
            .await;
        let read_token = fixture
            .token(
                "storage-client",
                fixture.user_id,
                vec![StorageAuthorizationAction::Read],
            )
            .await;

        let created = fixture
            .request(
                http::Method::POST,
                "/storage/databases",
                &write_token,
                Some(r#"{"name":"primary"}"#),
            )
            .await;
        assert_eq!(created.status(), StatusCode::CREATED);
        let created_body = to_bytes(created.into_body(), usize::MAX)
            .await
            .expect("read create response");
        let resource: DatabaseResource =
            serde_json::from_slice(&created_body).expect("decode created resource");
        let id = resource.id.to_string();
        let application_id = fixture
            .oauth2
            .application_id_for_client("storage-client")
            .await
            .expect("resolve persisted application")
            .to_string();
        assert_eq!(
            fixture
                .oauth2
                .application_id_for_client("same-application-client")
                .await
                .expect("resolve shared application")
                .to_string(),
            application_id
        );

        let listed = fixture
            .request(http::Method::GET, "/storage/databases", &read_token, None)
            .await;
        assert_eq!(listed.status(), StatusCode::OK);
        let listed_body = to_bytes(listed.into_body(), usize::MAX)
            .await
            .expect("read list response");
        let resources: Vec<DatabaseResource> =
            serde_json::from_slice(&listed_body).expect("decode listed resources");
        assert_eq!(resources, vec![resource.clone()]);

        let shared_app_token = fixture
            .token(
                "same-application-client",
                fixture.user_id,
                vec![StorageAuthorizationAction::Read],
            )
            .await;
        let shared_app_list = fixture
            .request(
                http::Method::GET,
                "/storage/databases",
                &shared_app_token,
                None,
            )
            .await;
        assert_eq!(shared_app_list.status(), StatusCode::OK);
        let body = to_bytes(shared_app_list.into_body(), usize::MAX)
            .await
            .expect("read same-application client list");
        let resources: Vec<DatabaseResource> =
            serde_json::from_slice(&body).expect("decode shared-application resources");
        assert_eq!(resources, vec![resource.clone()]);

        let fetched = fixture
            .request(
                http::Method::GET,
                &format!("/storage/databases/{id}"),
                &read_token,
                None,
            )
            .await;
        assert_eq!(fetched.status(), StatusCode::OK);
        let body = to_bytes(fetched.into_body(), usize::MAX)
            .await
            .expect("read database detail");
        let detail: serde_json::Value =
            serde_json::from_slice(&body).expect("decode database detail");
        assert_eq!(
            detail,
            serde_json::json!({"id": id, "name": "primary", "applicationId": application_id})
        );
        let shared_get = fixture
            .request(
                http::Method::GET,
                &format!("/storage/databases/{id}"),
                &shared_app_token,
                None,
            )
            .await;
        assert_eq!(shared_get.status(), StatusCode::OK);
        let body = to_bytes(shared_get.into_body(), usize::MAX)
            .await
            .expect("read shared database detail");
        let shared_detail: serde_json::Value =
            serde_json::from_slice(&body).expect("decode shared database detail");
        assert_eq!(shared_detail, detail);

        let denied_create = fixture
            .request(
                http::Method::POST,
                "/storage/databases",
                &read_token,
                Some(r#"{"name":"denied"}"#),
            )
            .await;
        assert_eq!(denied_create.status(), StatusCode::FORBIDDEN);
        let denied_delete = fixture
            .request(
                http::Method::DELETE,
                &format!("/storage/databases/{id}"),
                &read_token,
                None,
            )
            .await;
        assert_eq!(denied_delete.status(), StatusCode::FORBIDDEN);

        let other_subject_token = fixture
            .token(
                "storage-client",
                fixture.second_user_id,
                vec![StorageAuthorizationAction::Read],
            )
            .await;
        let other_subject_list = fixture
            .request(
                http::Method::GET,
                "/storage/databases",
                &other_subject_token,
                None,
            )
            .await;
        assert_eq!(other_subject_list.status(), StatusCode::OK);
        let body = to_bytes(other_subject_list.into_body(), usize::MAX)
            .await
            .expect("read other subject list");
        let resources: Vec<DatabaseResource> =
            serde_json::from_slice(&body).expect("decode other subject resources");
        assert!(resources.is_empty());

        let other_application_token = fixture
            .token(
                "other-client",
                fixture.user_id,
                vec![
                    StorageAuthorizationAction::Read,
                    StorageAuthorizationAction::Write,
                ],
            )
            .await;
        let other_application_list = fixture
            .request(
                http::Method::GET,
                "/storage/databases",
                &other_application_token,
                None,
            )
            .await;
        assert_eq!(other_application_list.status(), StatusCode::OK);
        let body = to_bytes(other_application_list.into_body(), usize::MAX)
            .await
            .expect("read other application list");
        let resources: Vec<DatabaseResource> =
            serde_json::from_slice(&body).expect("decode other application resources");
        assert!(resources.is_empty());

        let foreign_get = fixture
            .request(
                http::Method::GET,
                &format!("/storage/databases/{id}"),
                &other_application_token,
                None,
            )
            .await;
        assert_eq!(foreign_get.status(), StatusCode::NOT_FOUND);

        let foreign_delete = fixture
            .request(
                http::Method::DELETE,
                &format!("/storage/databases/{id}"),
                &other_application_token,
                None,
            )
            .await;
        assert_eq!(foreign_delete.status(), StatusCode::NOT_FOUND);

        let created_file_system = fixture
            .request(
                http::Method::POST,
                "/storage/filesystems",
                &write_token,
                Some(r#"{"name":"files"}"#),
            )
            .await;
        assert_eq!(created_file_system.status(), StatusCode::CREATED);
        let body = to_bytes(created_file_system.into_body(), usize::MAX)
            .await
            .expect("read filesystem create response");
        let file_system: serde_json::Value =
            serde_json::from_slice(&body).expect("decode filesystem resource");
        let file_system_id = file_system["id"]
            .as_str()
            .expect("filesystem ID is serialized as a string");

        let shared_app_file_systems = fixture
            .request(
                http::Method::GET,
                "/storage/filesystems",
                &shared_app_token,
                None,
            )
            .await;
        assert_eq!(shared_app_file_systems.status(), StatusCode::OK);
        let body = to_bytes(shared_app_file_systems.into_body(), usize::MAX)
            .await
            .expect("read same-application filesystem list");
        let resources: Vec<serde_json::Value> =
            serde_json::from_slice(&body).expect("decode shared-application filesystems");
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0]["id"], file_system_id);

        let file_system_get = fixture
            .request(
                http::Method::GET,
                &format!("/storage/filesystems/{file_system_id}"),
                &read_token,
                None,
            )
            .await;
        assert_eq!(file_system_get.status(), StatusCode::OK);
        let body = to_bytes(file_system_get.into_body(), usize::MAX)
            .await
            .expect("read filesystem detail");
        let detail: serde_json::Value =
            serde_json::from_slice(&body).expect("decode filesystem detail");
        assert_eq!(
            detail,
            serde_json::json!({"id": file_system_id, "name": "files", "applicationId": application_id})
        );
        let shared_file_system_get = fixture
            .request(
                http::Method::GET,
                &format!("/storage/filesystems/{file_system_id}"),
                &shared_app_token,
                None,
            )
            .await;
        assert_eq!(shared_file_system_get.status(), StatusCode::OK);
        let body = to_bytes(shared_file_system_get.into_body(), usize::MAX)
            .await
            .expect("read shared filesystem detail");
        let shared_detail: serde_json::Value =
            serde_json::from_slice(&body).expect("decode shared filesystem detail");
        assert_eq!(shared_detail, detail);

        let denied_file_system_create = fixture
            .request(
                http::Method::POST,
                "/storage/filesystems",
                &read_token,
                Some(r#"{"name":"denied"}"#),
            )
            .await;
        assert_eq!(denied_file_system_create.status(), StatusCode::FORBIDDEN);

        let foreign_file_system_get = fixture
            .request(
                http::Method::GET,
                &format!("/storage/filesystems/{file_system_id}"),
                &other_application_token,
                None,
            )
            .await;
        assert_eq!(foreign_file_system_get.status(), StatusCode::NOT_FOUND);

        let deleted_file_system = fixture
            .request(
                http::Method::DELETE,
                &format!("/storage/filesystems/{file_system_id}"),
                &write_token,
                None,
            )
            .await;
        assert_eq!(deleted_file_system.status(), StatusCode::NO_CONTENT);
        let tombstoned_file_system = fixture
            .request(
                http::Method::GET,
                &format!("/storage/filesystems/{file_system_id}"),
                &read_token,
                None,
            )
            .await;
        assert_eq!(tombstoned_file_system.status(), StatusCode::NOT_FOUND);

        let deleted = fixture
            .request(
                http::Method::DELETE,
                &format!("/storage/databases/{id}"),
                &write_token,
                None,
            )
            .await;
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
        let tombstoned_database = fixture
            .request(
                http::Method::GET,
                &format!("/storage/databases/{id}"),
                &read_token,
                None,
            )
            .await;
        assert_eq!(tombstoned_database.status(), StatusCode::NOT_FOUND);
        fixture._endpoint.close().await;
        std::fs::remove_dir_all(fixture.temp_dir).expect("remove fixture directory");
    }

    fn claims(
        resource: Option<&str>,
        audience: &str,
        actions: Vec<StorageAuthorizationAction>,
    ) -> StandardClaims {
        StandardClaims {
            r#type: TokenType::Bearer,
            r#use: TokenUse::Access,
            exp: 1,
            iat: 0,
            nbf: 0,
            iss: "issuer".into(),
            aud: audience.into(),
            client_id: "client".into(),
            sub: "subject".into(),
            resource: resource.map(str::to_owned),
            authorization_details: Some(vec![AuthorizationDetail::Storage(
                StorageAuthorizationDetail { actions },
            )]),
            scope: Vec::new(),
        }
    }

    #[test]
    fn namespace_uses_subject_and_application_not_client_id() {
        let application_id = idp_model::model::Id::now_v7();
        let mut first_client = claims(Some("storage"), "storage", vec![]);
        first_client.client_id = "client-one".into();
        let mut second_client = first_client.clone();
        second_client.client_id = "client-two".into();

        let first_namespace = storage_namespace(&first_client, application_id);
        let second_namespace = storage_namespace(&second_client, application_id);
        assert_eq!(first_namespace, second_namespace);

        let mut other_subject = claims(Some("storage"), "storage", vec![]);
        other_subject.sub = "other-subject".into();
        assert_ne!(
            first_namespace,
            storage_namespace(&other_subject, application_id)
        );
        assert_ne!(
            first_namespace,
            storage_namespace(&first_client, idp_model::model::Id::now_v7())
        );
    }

    #[test]
    fn storage_access_requires_matching_resource_audience_and_single_detail() {
        assert_eq!(
            storage_access(&claims(
                Some("storage"),
                "storage",
                vec![StorageAuthorizationAction::Read]
            )),
            Some((true, false))
        );
        assert_eq!(
            storage_access(&claims(
                Some("storage"),
                "storage",
                vec![StorageAuthorizationAction::Write]
            )),
            Some((false, true))
        );
        assert_eq!(
            storage_access(&claims(
                Some("storage"),
                "other",
                vec![StorageAuthorizationAction::Read]
            )),
            None
        );
        assert_eq!(
            storage_access(&claims(
                None,
                "storage",
                vec![StorageAuthorizationAction::Read]
            )),
            None
        );

        let mut multiple_details = claims(
            Some("storage"),
            "storage",
            vec![StorageAuthorizationAction::Read],
        );
        multiple_details
            .authorization_details
            .as_mut()
            .expect("details are present")
            .push(AuthorizationDetail::Storage(StorageAuthorizationDetail {
                actions: vec![StorageAuthorizationAction::Write],
            }));
        assert_eq!(storage_access(&multiple_details), None);
    }
}
