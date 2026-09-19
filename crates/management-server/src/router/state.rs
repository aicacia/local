use std::sync::Arc;

use idp_service::{
    oauth2::OAuth2Service,
    replica::{
        DbApplicationRepo, DbClientRepo, DbKeyRepo, DbOAuth2AuthorizationCodeRepo,
        DbOAuth2UserConsentRepo, DbUserRepo,
    },
};
use management_service::{
    ManagementService,
    replica::{DbPermissionRepo, DbRoleRepo},
};
use sync_db::{AutomergeRowCodec, RedbKernel};

type DbApplication = DbApplicationRepo<RedbKernel, AutomergeRowCodec>;
type DbClient = DbClientRepo<RedbKernel, AutomergeRowCodec>;
type DbKey = DbKeyRepo<RedbKernel, AutomergeRowCodec>;
type DbAuthorizationCode = DbOAuth2AuthorizationCodeRepo<RedbKernel, AutomergeRowCodec>;
type DbUser = DbUserRepo<RedbKernel, AutomergeRowCodec>;
type DbUserConsent = DbOAuth2UserConsentRepo<RedbKernel, AutomergeRowCodec>;
type DbPermission = DbPermissionRepo<RedbKernel, AutomergeRowCodec>;
type DbRole = DbRoleRepo<RedbKernel, AutomergeRowCodec>;

pub(crate) type ManagementRouterService = ManagementService<DbApplication, DbPermission, DbRole>;

type LocalOAuth2Service =
    OAuth2Service<DbApplication, DbClient, DbAuthorizationCode, DbUser, DbUserConsent, DbKey>;

#[derive(Clone)]
pub struct RouterState {
    pub(crate) api_base_uri: String,
    pub(crate) management_service: Arc<ManagementRouterService>,
    pub(crate) oauth2_service: Arc<LocalOAuth2Service>,
}

impl RouterState {
    pub fn new(
        api_base_uri: impl Into<String>,
        management_service: Arc<ManagementRouterService>,
        oauth2_service: Arc<LocalOAuth2Service>,
    ) -> Self {
        Self {
            api_base_uri: api_base_uri.into(),
            management_service,
            oauth2_service,
        }
    }
}
