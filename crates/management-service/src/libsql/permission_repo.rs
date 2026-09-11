use std::sync::Arc;

use idp_model::model::Permission;
use libsql::{Database, de::from_row};

use crate::{ManagementError, ManagementResult};

use crate::PermissionRepo;

pub struct LibSqlPermissionRepo {
    database: Arc<Database>,
}

impl LibSqlPermissionRepo {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

impl PermissionRepo for LibSqlPermissionRepo {
    async fn list_permissions(
        &self,
        application_id: i64,
        offset: u32,
        limit: u32,
    ) -> ManagementResult<Vec<Permission>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                application_id,
                name,
                description,
                created_at,
                updated_at
            FROM permissions
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
            roles.push(from_row::<Permission>(&row)?);
        }

        Ok(roles)
    }

    async fn create_permission(
        &self,
        application_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> ManagementResult<Permission> {
        let connection = self.database.connect()?;
        let query = r#"
            INSERT INTO permissions (application_id, name, description)
            VALUES (?, ?, ?)
            RETURNING id,
                application_id,
                name,
                description,
                created_at,
                updated_at;
        "#;
        let mut rows = connection
            .query(query, libsql::params![application_id, name, description])
            .await?;
        if let Some(row) = rows.next().await? {
            Ok(from_row::<Permission>(&row)?)
        } else {
            Err(ManagementError::LibSql(libsql::Error::QueryReturnedNoRows))
        }
    }

    async fn find_permission_by_id(
        &self,
        application_id: i64,
        permission_id: i64,
    ) -> ManagementResult<Option<Permission>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                application_id,
                name,
                description,
                created_at,
                updated_at
            FROM permissions
            WHERE application_id = ? AND id = ?
        "#;
        let mut rows = connection
            .query(query, libsql::params![application_id, permission_id])
            .await?;
        if let Some(row) = rows.next().await? {
            Ok(Some(from_row::<Permission>(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn delete_permission_by_id(
        &self,
        application_id: i64,
        permission_id: i64,
    ) -> ManagementResult<()> {
        let connection = self.database.connect()?;
        let query = r#"
            DELETE FROM permissions
            WHERE application_id = ? AND id = ?
        "#;
        let result = connection
            .execute(query, libsql::params![application_id, permission_id])
            .await?;
        if result == 0 {
            Err(ManagementError::LibSql(libsql::Error::QueryReturnedNoRows))
        } else {
            Ok(())
        }
    }

    async fn add_permission_to_role(
        &self,
        application_id: i64,
        role_id: i64,
        permission_id: i64,
    ) -> ManagementResult<()> {
        let connection = self.database.connect()?;
        let query = r#"
            INSERT INTO role_permissions (role_id, permission_id)
            SELECT r.id, p.id
            FROM roles r
            INNER JOIN permissions p ON p.id = ?
            WHERE r.id = ?
                AND r.application_id = ?
                AND p.application_id = ?;
        "#;
        let result = connection
            .execute(
                query,
                libsql::params![permission_id, role_id, application_id, application_id],
            )
            .await?;
        if result == 0 {
            Err(ManagementError::LibSql(libsql::Error::QueryReturnedNoRows))
        } else {
            Ok(())
        }
    }

    async fn remove_permission_from_role(
        &self,
        application_id: i64,
        role_id: i64,
        permission_id: i64,
    ) -> ManagementResult<()> {
        let connection = self.database.connect()?;
        let query = r#"
            DELETE FROM role_permissions
            WHERE role_id = ?
                AND permission_id = ?
                AND EXISTS (
                    SELECT 1
                    FROM roles r
                    INNER JOIN permissions p ON p.id = role_permissions.permission_id
                    WHERE r.id = role_permissions.role_id
                        AND r.application_id = ?
                        AND p.application_id = ?
                )
        "#;
        let result = connection
            .execute(
                query,
                libsql::params![role_id, permission_id, application_id, application_id],
            )
            .await?;
        if result == 0 {
            Err(ManagementError::LibSql(libsql::Error::QueryReturnedNoRows))
        } else {
            Ok(())
        }
    }

    async fn list_role_permissions(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> ManagementResult<Vec<Permission>> {
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
            INNER JOIN roles r ON r.id = rp.role_id
            WHERE rp.role_id = ?
                AND r.application_id = ?
                AND p.application_id = ?
            ORDER BY p.name ASC
        "#;
        let mut rows = connection
            .query(
                query,
                libsql::params![role_id, application_id, application_id],
            )
            .await?;
        let mut permissions = Vec::new();
        while let Some(row) = rows.next().await? {
            permissions.push(from_row::<Permission>(&row)?);
        }
        Ok(permissions)
    }
}
