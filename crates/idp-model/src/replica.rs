use include_dir::{Dir, include_dir};

use db::{
    Engine, EngineResult, Kernel, RowCodec,
    migrate::{MigrationFile, replica_up},
};

static MIGRATIONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/replica_migrations");

pub async fn up<K, R>(engine: &Engine<K, R>) -> EngineResult<()>
where
    K: Kernel,
    R: RowCodec<K::Transaction>,
{
    replica_up(engine, &migration_files()).await
}

fn migration_files() -> Vec<MigrationFile> {
    MIGRATIONS
        .files()
        .map(|file| MigrationFile {
            name: file.path().to_string_lossy().into_owned(),
            contents: file
                .contents_utf8()
                .expect("replica migration must be UTF-8")
                .to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use db::{AutomergeRowCodec, Engine, InMemoryKernel, SqlTranslator};
    use futures::executor::block_on;

    use super::up;

    #[test]
    fn initializes_idempotently_with_uuid_primary_keys_and_unique_devices() {
        block_on(async {
            let engine = Engine::new(InMemoryKernel::new(), AutomergeRowCodec::new());
            up(&engine).await.unwrap();
            up(&engine).await.unwrap();

            for table in [
                "users",
                "user_emails",
                "user_phone_numbers",
                "credentials",
                "keys",
                "applications",
                "clients",
                "devices",
                "roles",
                "permissions",
                "role_permissions",
                "application_user_roles",
                "oauth2_authorization_codes",
                "oauth2_user_consents",
                "oauth2_refresh_tokens",
            ] {
                let schema = engine.table_schema(table).await.unwrap();
                assert_eq!(schema.columns[0].name, "id");
                assert_eq!(
                    schema
                        .columns
                        .iter()
                        .filter(|column| column.primary_key)
                        .count(),
                    1
                );
            }

            engine
                .translate_and_execute(
                    "INSERT INTO devices (id, name, public_key, address, state, approved_at, revoked_at, created_at, updated_at) VALUES (CAST('00000000-0000-0000-0000-000000000001' AS UUID), 'one', 'key', 'address', 1, NULL, NULL, 0, 0)",
                    &SqlTranslator,
                )
                .await
                .unwrap();
            assert!(engine
                .translate_and_execute(
                    "INSERT INTO devices (id, name, public_key, address, state, approved_at, revoked_at, created_at, updated_at) VALUES (CAST('00000000-0000-0000-0000-000000000002' AS UUID), 'two', 'key', 'address', 1, NULL, NULL, 0, 0)",
                    &SqlTranslator,
                )
                .await
                .is_err());
        });
    }
}
