use crate::{IdpClient, ManagementClient};

#[derive(Clone)]
pub struct RouterState {
    pub api_base_uri: String,
    pub idp_client: Option<IdpClient>,
    pub management_client: Option<ManagementClient>,
}

impl RouterState {
    pub fn new(api_base_uri: impl Into<String>) -> Self {
        Self {
            api_base_uri: api_base_uri.into(),
            idp_client: None,
            management_client: None,
        }
    }

    pub fn with_idp_client(mut self, idp_client: IdpClient) -> Self {
        self.idp_client = Some(idp_client);
        self
    }

    pub fn with_management_client(mut self, management_client: ManagementClient) -> Self {
        self.management_client = Some(management_client);
        self
    }
}
