use std::{path::Path, sync::Arc};

use converge::{AutomergeRowCodec, Engine, RedbKernel, redb};

pub type NativeEngine = Engine<RedbKernel, AutomergeRowCodec>;

pub fn open_native_engine(path: impl AsRef<Path>) -> Result<NativeEngine, redb::Error> {
    let database = Arc::new(redb::Database::create(path)?);
    Ok(Engine::new(
        RedbKernel::new(database),
        AutomergeRowCodec::new(),
    ))
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::open_native_engine;

    #[test]
    fn opens_a_durable_engine() {
        let path = std::env::temp_dir().join(format!(
            "idp-db-{}-{}.redb",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine = open_native_engine(&path).unwrap();

        drop(engine);
        std::fs::remove_file(path).unwrap();
    }
}
