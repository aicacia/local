use std::sync::Arc;

use idp_model::model::Application;
use libsql::{Database, de::from_row};

use idp_service::repo::{ApplicationRepo, RepoError, RepoResult};

pub struct LibSqlApplicationRepo {
    database: Arc<Database>,
}

impl LibSqlApplicationRepo {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

impl ApplicationRepo for LibSqlApplicationRepo {
    async fn find_by_id(&self, application_id: i64) -> RepoResult<Option<Application>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT id, name, uri, description, created_at, updated_at
            FROM applications
            WHERE id = ?
        "#;
        let row = {
            let mut rows = connection
                .query(query, libsql::params![application_id])
                .await?;
            if let Some(row) = rows.next().await? {
                row
            } else {
                return Ok(None);
            }
        };

        let client = from_row::<Application>(&row)?;
        Ok(Some(client))
    }

    async fn find_by_uri(&self, uri: &str) -> RepoResult<Option<Application>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT id, name, uri, description, created_at, updated_at
            FROM applications
            WHERE uri = ?
        "#;
        let row = {
            let mut rows = connection.query(query, libsql::params![uri]).await?;
            if let Some(row) = rows.next().await? {
                row
            } else {
                return Ok(None);
            }
        };

        let client = from_row::<Application>(&row)?;
        Ok(Some(client))
    }

    async fn list_applications(&self, offset: u32, limit: u32) -> RepoResult<Vec<Application>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT id, name, uri, description, created_at, updated_at
            FROM applications
            LIMIT ? OFFSET ?
        "#;
        let mut rows = connection
            .query(query, libsql::params![limit, offset])
            .await?;

        let mut applications = Vec::new();
        while let Some(row) = rows.next().await? {
            applications.push(from_row::<Application>(&row)?);
        }

        Ok(applications)
    }

    async fn create_application(
        &self,
        name: String,
        uri: String,
        description: Option<String>,
    ) -> RepoResult<Application> {
        let connection = self.database.connect()?;
        let query = r#"
            INSERT INTO applications (name, uri, description)
            VALUES (?, ?, ?)
            RETURNING id, name, uri, description, created_at, updated_at
        "#;
        let row = {
            let mut rows = connection
                .query(query, libsql::params![name, uri, description])
                .await?;

            if let Some(row) = rows.next().await? {
                row
            } else {
                return Err(RepoError::LibSql(libsql::Error::QueryReturnedNoRows));
            }
        };

        let application = from_row::<Application>(&row)?;
        Ok(application)
    }

    async fn update_application(&self, application: Application) -> RepoResult<Application> {
        let connection = self.database.connect()?;
        let query = r#"
            UPDATE applications
            SET name = ?, uri = ?, description = ?, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            RETURNING id, name, uri, description, created_at, updated_at
        "#;
        let row = {
            let mut rows = connection
                .query(
                    query,
                    libsql::params![
                        application.name,
                        application.uri,
                        application.description,
                        application.id
                    ],
                )
                .await?;

            if let Some(row) = rows.next().await? {
                row
            } else {
                return Err(RepoError::LibSql(libsql::Error::QueryReturnedNoRows));
            }
        };

        let updated_application = from_row::<Application>(&row)?;
        Ok(updated_application)
    }

    async fn delete_application_by_id(&self, application_id: i64) -> RepoResult<()> {
        let connection = self.database.connect()?;
        let query = r#"
            DELETE FROM applications
            WHERE id = ?
        "#;
        let result = connection
            .execute(query, libsql::params![application_id])
            .await?;

        if result == 0 {
            return Err(RepoError::LibSql(libsql::Error::QueryReturnedNoRows));
        }

        Ok(())
    }
}
