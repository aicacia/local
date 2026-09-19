#[cfg(feature = "cli")]
pub mod cli;
mod database;
mod database_config;
mod helpers;
#[cfg(feature = "migrate")]
pub mod migrate;

#[cfg(feature = "cli")]
pub use cli::run;
pub use database::{close_database, open_database};
pub use database_config::DatabaseConfig;
pub use helpers::run_transaction;
#[cfg(feature = "replica")]
pub use sync_db::{
    AutomergeRowCodec, Engine, EngineResult, InMemoryKernel, Kernel, RowCodec, SqlTranslator,
};
