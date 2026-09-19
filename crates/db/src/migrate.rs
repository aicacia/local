use std::collections::BTreeMap;

#[cfg(feature = "replica")]
mod replica;
#[cfg(feature = "replica")]
pub use replica::up as replica_up;

use chrono::{DateTime, Utc};
use libsql::{Connection, Database};

#[derive(Clone, Debug)]
pub struct MigrationFile {
    pub name: String,
    pub contents: String,
}

struct MigrationState {
    pub name: String,
    pub version: String,
    pub up: Option<String>,
    pub down: Option<String>,
    pub applied_at: Option<DateTime<Utc>>,
}

pub async fn up(database: &Database, files: &[MigrationFile]) -> libsql::Result<()> {
    let connection = database.connect()?;

    let migrations = migration_states(&connection, files).await?;

    let tx = connection.transaction().await?;

    for migration in migrations {
        if let Some(applied_at) = migration.applied_at {
            log::info!(
                "Migration {} (version {}) already applied at {}",
                migration.name,
                migration.version,
                applied_at
            );
        } else if let Some(sql) = migration.up {
            log::info!(
                "Applying migration {} (version {})",
                migration.name,
                migration.version
            );
            tx.execute_batch(&sql).await?;
            tx.execute(
                "INSERT INTO __migrations (name, version, applied_at) VALUES (?, ?, ?);",
                libsql::params![migration.name, migration.version, Utc::now().timestamp()],
            )
            .await?;
        } else {
            log::info!(
                "Migration {} (version {}) has no up migration, skipping",
                migration.name,
                migration.version
            );
        }
    }

    tx.commit().await?;

    Ok(())
}

pub async fn down(database: &Database, files: &[MigrationFile]) -> libsql::Result<()> {
    let connection = database.connect()?;

    let mut migrations = migration_states(&connection, files).await?;

    let tx = connection.transaction().await?;

    while let Some(migration) = migrations.pop() {
        let mut ran = false;

        if let Some(sql) = migration.down {
            log::info!(
                "Reverting migration {} (version {})",
                migration.name,
                migration.version
            );
            tx.execute_batch(&sql).await?;
            ran = true;
        } else {
            log::info!(
                "Migration {} (version {}) no down migration found, skipping",
                migration.name,
                migration.version
            );
        }
        tx.execute(
            "DELETE FROM __migrations WHERE name = ? AND version = ?;",
            libsql::params![migration.name, migration.version],
        )
        .await?;

        if ran {
            break;
        }
    }

    tx.commit().await?;

    Ok(())
}

async fn migration_states(
    connection: &Connection,
    files: &[MigrationFile],
) -> libsql::Result<Vec<MigrationState>> {
    connection
        .execute(
            r#"CREATE TABLE IF NOT EXISTS __migrations (
            name TEXT PRIMARY KEY,
            version TEXT NOT NULL,
            applied_at INTEGER NOT NULL
        );"#,
            libsql::params![],
        )
        .await?;

    let mut rows = connection
        .query(
            r#"SELECT name, version, applied_at FROM __migrations ORDER BY applied_at DESC;"#,
            libsql::params![],
        )
        .await?;

    let mut migrations = BTreeMap::new();

    while let Some(row) = rows.next().await? {
        let name: String = row.get(0)?;
        let version: String = row.get(1)?;
        let applied_at = DateTime::from_timestamp_secs(row.get(2)?).unwrap_or_else(Utc::now);

        migrations.insert(
            name.clone(),
            MigrationState {
                name,
                version,
                up: None,
                down: None,
                applied_at: Some(applied_at),
            },
        );
    }

    for MigrationFile { name, contents } in files {
        let is_up = name.contains(".up.");
        let is_down = name.contains(".down.");

        if !is_up && !is_down {
            return Err(libsql::Error::Misuse(format!(
                "Migration file {} must contain either .up. or .down. in its name",
                name
            )));
        }
        if is_up && is_down {
            return Err(libsql::Error::Misuse(format!(
                "Migration file {} cannot contain both .up. and .down. in its name",
                name
            )));
        }
        let name = name
            .trim_end_matches(".up.sql")
            .trim_end_matches(".down.sql");

        if let Some(applied_migration) = migrations.get_mut(name) {
            if is_up {
                applied_migration.up = Some(contents.clone());
            } else if is_down {
                applied_migration.down = Some(contents.clone());
            }
        } else {
            let (up, down) = if is_up {
                (Some(contents.clone()), None)
            } else {
                (None, Some(contents.clone()))
            };

            migrations.insert(
                name.to_owned(),
                MigrationState {
                    name: name.to_owned(),
                    version: "".to_owned(), // defer to second loop to get both up and down contents
                    up,
                    down,
                    applied_at: None,
                },
            );
        }
    }

    for migration in migrations.values_mut() {
        let mut combined_contents = String::new();
        if let Some(up) = &migration.up {
            combined_contents.push_str(up);
        }
        if let Some(down) = &migration.down {
            combined_contents.push_str(down);
        }
        migration.version = bytes_to_sha256(&combined_contents);
    }

    Ok(migrations.into_values().collect())
}

fn bytes_to_sha256(bytes: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}
