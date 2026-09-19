use sync_db::{Engine, EngineResult, Kernel, RowCodec, SqlTranslator};

use super::MigrationFile;

pub async fn up<K, R>(engine: &Engine<K, R>, files: &[MigrationFile]) -> EngineResult<()>
where
    K: Kernel,
    R: RowCodec<K::Transaction>,
{
    let mut files = files
        .iter()
        .filter(|file| file.name.contains(".up."))
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.name.cmp(&right.name));

    for file in files {
        engine
            .translate_and_execute(&file.contents, &SqlTranslator)
            .await?;
    }
    Ok(())
}
