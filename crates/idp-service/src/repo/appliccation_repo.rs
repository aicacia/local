use idp_model::model::{Application, Id};

use crate::repo::RepoResult;

pub trait ApplicationRepo {
    fn find_by_id(
        &self,
        application_id: Id,
    ) -> impl Future<Output = RepoResult<Option<Application>>>;

    fn find_by_uri(&self, uri: &str) -> impl Future<Output = RepoResult<Option<Application>>>;

    fn list_applications(
        &self,
        offset: u32,
        limit: u32,
    ) -> impl Future<Output = RepoResult<Vec<Application>>>;

    fn create_application(
        &self,
        name: String,
        uri: String,
        description: Option<String>,
    ) -> impl Future<Output = RepoResult<Application>>;

    fn update_application(
        &self,
        application: Application,
    ) -> impl Future<Output = RepoResult<Application>>;

    fn delete_application_by_id(&self, application_id: Id) -> impl Future<Output = RepoResult<()>>;
}
