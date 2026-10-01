use std::sync::Arc;

use idp_service::{
    oauth2::OAuth2Service,
    replica::{
        DbApplicationRepo, DbClientRepo, DbKeyRepo, DbOAuth2AuthorizationCodeRepo,
        DbOAuth2UserConsentRepo, DbUserRepo,
    },
    repo::PrivateKeyKeyringRepo,
};
use management_service::{
    HostedControlPlane, ManagementService,
    replica::{DbDeviceRepo, DbPermissionRepo, DbRoleRepo, DbSelectionPolicyRepo},
};
use ofdb::{AutomergeRowCodec, RedbKernel};

type DbApplication = DbApplicationRepo<RedbKernel, AutomergeRowCodec>;
type DbClient = DbClientRepo<RedbKernel, AutomergeRowCodec>;
type DbKey = DbKeyRepo<RedbKernel, AutomergeRowCodec>;
type DbAuthorizationCode = DbOAuth2AuthorizationCodeRepo<RedbKernel, AutomergeRowCodec>;
type DbUser = DbUserRepo<RedbKernel, AutomergeRowCodec>;
type DbUserConsent = DbOAuth2UserConsentRepo<RedbKernel, AutomergeRowCodec>;
type DbPermission = DbPermissionRepo<RedbKernel, AutomergeRowCodec>;
type DbRole = DbRoleRepo<RedbKernel, AutomergeRowCodec>;
type DbSelectionPolicy = DbSelectionPolicyRepo<RedbKernel, AutomergeRowCodec>;
type DbDevice = DbDeviceRepo<RedbKernel, AutomergeRowCodec>;

pub(crate) type ManagementRouterService = ManagementService<DbApplication, DbPermission, DbRole>;

type LocalOAuth2Service = OAuth2Service<
    DbApplication,
    DbClient,
    DbAuthorizationCode,
    DbUser,
    DbUserConsent,
    DbKey,
    PrivateKeyKeyringRepo,
>;

#[derive(Clone)]
pub struct RouterState {
    pub(crate) api_base_uri: String,
    pub(crate) management_service: Arc<ManagementRouterService>,
    pub(crate) oauth2_service: Arc<LocalOAuth2Service>,
    pub(crate) selection_policies: Arc<DbSelectionPolicy>,
    pub(crate) devices: Option<Arc<DbDevice>>,
    pub(crate) control_plane: Arc<HostedControlPlane>,
    pub(crate) storage_audience: String,
}

impl RouterState {
    pub fn new(
        api_base_uri: impl Into<String>,
        management_service: Arc<ManagementRouterService>,
        oauth2_service: Arc<LocalOAuth2Service>,
        selection_policies: Arc<DbSelectionPolicy>,
        control_plane: Arc<HostedControlPlane>,
        storage_audience: impl Into<String>,
    ) -> Self {
        Self {
            api_base_uri: api_base_uri.into(),
            management_service,
            oauth2_service,
            selection_policies,
            devices: None,
            control_plane,
            storage_audience: storage_audience.into(),
        }
    }

    pub fn with_devices(mut self, devices: Arc<DbDevice>) -> Self {
        self.devices = Some(devices);
        self
    }
}
