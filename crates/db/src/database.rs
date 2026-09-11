use std::{fs::create_dir_all, path::Path};

use libsql::OpenFlags;
use url::Url;

use crate::DatabaseConfig;

pub async fn open_database(database_config: &DatabaseConfig) -> libsql::Result<libsql::Database> {
    if database_config.url == ":memory:" {
        return libsql::Builder::new_local(":memory:").build().await;
    }
    let database_url =
        Url::parse(&database_config.url).map_err(|e| libsql::Error::Misuse(e.to_string()))?;

    log::info!("initializing sqlite database: {}", database_url);
    match database_url.scheme() {
        "file" | "sqlite" => {
            let database_path = local_database_path(&database_url, &database_config.url);
            let path = Path::new(database_path);
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
                && !parent.exists()
            {
                log::info!("Creating database directory: {:?}", parent);
                match create_dir_all(parent) {
                    Ok(_) => (),
                    Err(e) => {
                        return Err(libsql::Error::Misuse(e.to_string()));
                    }
                }
            }

            libsql::Builder::new_local(database_path)
                .flags(OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_READ_WRITE)
                .build()
                .await
        }
        #[cfg(feature = "remote")]
        "libsql" => {
            libsql::Builder::new_remote(
                database_url.to_string(),
                database_config.auth_token.clone().unwrap_or_default(),
            )
            .build()
            .await
        }
        _ => Err(libsql::Error::Misuse(format!(
            "unsupported database scheme: {}",
            database_url.scheme()
        ))),
    }
}

fn local_database_path<'a>(database_url: &'a Url, database_config_url: &'a str) -> &'a str {
    database_config_url
        .strip_prefix("file://")
        .or_else(|| database_config_url.strip_prefix("sqlite://"))
        .unwrap_or(database_url.path())
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::local_database_path;

    #[test]
    fn preserves_relative_file_url_paths() {
        let database_config_url = "file://./config/primary/database.db";
        let database_url = Url::parse(database_config_url).unwrap();

        assert_eq!(
            local_database_path(&database_url, database_config_url),
            "./config/primary/database.db"
        );
    }
}

pub async fn close_database(database: &libsql::Database) -> libsql::Result<()> {
    database
        .connect()?
        .execute_batch("PRAGMA analysis_limit=400; PRAGMA optimize;")
        .await?;
    Ok(())
}
