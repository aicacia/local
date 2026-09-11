use std::sync::Arc;

use idp_model::model::Role;
use libsql::{Database, de::from_row};

use crate::repo::{RepoResult, RoleRepo};

pub struct LibSqlRoleRepo {
    database: Arc<Database>,
}

impl LibSqlRoleRepo {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

fn permission_matches(granted: &str, required: &str) -> bool {
    if granted == "*" || granted == required {
        return true;
    }

    if let Some(prefix) = granted.strip_suffix('*') {
        if required.starts_with(prefix) {
            return true;
        }

        if let Some(colon_prefix) = prefix.strip_suffix(':') {
            let dot_prefix = format!("{colon_prefix}.");
            return required.starts_with(&dot_prefix);
        }

        if let Some(dot_prefix) = prefix.strip_suffix('.') {
            let colon_prefix = format!("{dot_prefix}:");
            return required.starts_with(&colon_prefix);
        }
    }

    false
}

impl RoleRepo for LibSqlRoleRepo {
    async fn list_roles(
        &self,
        application_id: i64,
        offset: u32,
        limit: u32,
    ) -> RepoResult<Vec<Role>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                application_id,
                name,
                description,
                created_at,
                updated_at
            FROM roles
            WHERE application_id = ?
            ORDER BY name ASC
            LIMIT ? OFFSET ?
        "#;

        let mut rows = connection
            .query(
                query,
                libsql::params![application_id, i64::from(limit), i64::from(offset)],
            )
            .await?;
        let mut roles = Vec::new();

        while let Some(row) = rows.next().await? {
            roles.push(from_row::<Role>(&row)?);
        }

        Ok(roles)
    }

    async fn create_role(
        &self,
        application_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> RepoResult<Role> {
        let connection = self.database.connect()?;
        let query = r#"
            INSERT INTO roles (application_id, name, description)
            VALUES (?, ?, ?)
            RETURNING
                id,
                application_id,
                name,
                description,
                created_at,
                updated_at
        "#;

        let mut rows = connection
            .query(query, libsql::params![application_id, name, description])
            .await?;

        let row = rows
            .next()
            .await?
            .ok_or_else(|| libsql::Error::Misuse("role not found after insert".into()))?;

        Ok(from_row::<Role>(&row)?)
    }

    async fn find_role_by_id(&self, application_id: i64, role_id: i64) -> RepoResult<Option<Role>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                application_id,
                name,
                description,
                created_at,
                updated_at
            FROM roles
            WHERE application_id = ? AND id = ?
        "#;

        let row = {
            let mut rows = connection
                .query(query, libsql::params![application_id, role_id])
                .await?;
            if let Some(row) = rows.next().await? {
                row
            } else {
                return Ok(None);
            }
        };

        Ok(Some(from_row::<Role>(&row)?))
    }

    async fn delete_role_by_id(&self, application_id: i64, role_id: i64) -> RepoResult<()> {
        let connection = self.database.connect()?;
        connection
            .execute(
                "DELETE FROM roles WHERE application_id = ? AND id = ?",
                libsql::params![application_id, role_id],
            )
            .await?;
        Ok(())
    }

    async fn add_role_to_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> RepoResult<()> {
        let connection = self.database.connect()?;
        connection
            .execute(
                "INSERT INTO application_user_roles (application_id, user_id, role_id) VALUES (?, ?, ?)",
                libsql::params![application_id, user_id, role_id],
            )
            .await?;
        Ok(())
    }

    async fn remove_role_from_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> RepoResult<()> {
        let connection = self.database.connect()?;
        connection
            .execute(
                "DELETE FROM application_user_roles WHERE application_id = ? AND user_id = ? AND role_id = ?",
                libsql::params![application_id, user_id, role_id],
            )
            .await?;
        Ok(())
    }

    async fn list_user_roles(&self, application_id: i64, user_id: i64) -> RepoResult<Vec<Role>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                r.id,
                r.application_id,
                r.name,
                r.description,
                r.created_at,
                r.updated_at
            FROM roles r
            INNER JOIN application_user_roles aur ON r.id = aur.role_id
            WHERE aur.application_id = ? AND aur.user_id = ?
            ORDER BY r.name ASC
        "#;

        let mut rows = connection
            .query(query, libsql::params![application_id, user_id])
            .await?;
        let mut roles = Vec::new();

        while let Some(row) = rows.next().await? {
            roles.push(from_row::<Role>(&row)?);
        }

        Ok(roles)
    }

    async fn list_user_roles_across_applications(&self, user_id: i64) -> RepoResult<Vec<Role>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                r.id,
                r.application_id,
                r.name,
                r.description,
                r.created_at,
                r.updated_at
            FROM roles r
            INNER JOIN application_user_roles aur ON r.id = aur.role_id
            WHERE aur.user_id = ?
                AND aur.application_id = r.application_id
            ORDER BY r.application_id ASC, r.name ASC
        "#;

        let mut rows = connection.query(query, libsql::params![user_id]).await?;
        let mut roles = Vec::new();

        while let Some(row) = rows.next().await? {
            roles.push(from_row::<Role>(&row)?);
        }

        Ok(roles)
    }

    async fn list_user_permissions(
        &self,
        application_id: i64,
        user_id: i64,
    ) -> RepoResult<Vec<idp_model::model::Permission>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                p.id,
                p.application_id,
                p.name,
                p.description,
                p.created_at,
                p.updated_at
            FROM permissions p
            INNER JOIN role_permissions rp ON p.id = rp.permission_id
            INNER JOIN application_user_roles aur ON rp.role_id = aur.role_id
            WHERE aur.application_id = ? AND aur.user_id = ?
                AND p.application_id = aur.application_id
            ORDER BY p.name ASC
        "#;

        let mut rows = connection
            .query(query, libsql::params![application_id, user_id])
            .await?;
        let mut permissions = Vec::new();

        while let Some(row) = rows.next().await? {
            permissions.push(from_row::<idp_model::model::Permission>(&row)?);
        }

        Ok(permissions)
    }

    async fn has_user_client_permission(
        &self,
        user_id: i64,
        application_uri: &str,
        permission_name: &str,
    ) -> RepoResult<bool> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT p.name
            FROM application_user_roles aur
            INNER JOIN role_permissions rp ON aur.role_id = rp.role_id
            INNER JOIN permissions p ON rp.permission_id = p.id
            INNER JOIN applications a ON aur.application_id = a.id
            WHERE aur.user_id = ? AND a.uri = ?
        "#;

        let mut rows = connection
            .query(query, libsql::params![user_id, application_uri])
            .await?;

        while let Some(row) = rows.next().await? {
            let granted_permission = row.get::<String>(0)?;

            if permission_matches(&granted_permission, permission_name) {
                return Ok(true);
            }
        }

        Ok(false)
    }
}
