use include_dir::{Dir, include_dir};

use db::migrate::MigrationFile;

static MIGRATIONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/migrations");

pub async fn up(database: &libsql::Database) -> libsql::Result<()> {
    db::migrate::up(database, &migration_files()).await?;
    Ok(())
}

pub async fn down(database: &libsql::Database) -> libsql::Result<()> {
    db::migrate::down(database, &migration_files()).await?;
    Ok(())
}

fn migration_files() -> Vec<MigrationFile> {
    let mut files = Vec::new();

    for file in MIGRATIONS.files() {
        files.push(MigrationFile {
            name: file.path().to_str().expect("Invalid file path").to_string(),
            contents: file
                .contents_utf8()
                .expect("Invalid UTF-8 in file")
                .to_string(),
        });
    }

    files
}
