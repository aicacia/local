#[cfg(feature = "native")]
mod native;

#[cfg(feature = "native")]
pub use native::{NativeEngine, open_native_engine};
#[cfg(feature = "replica")]
pub use ofdb::{
    AutomergeRowCodec, Engine, EngineError, EngineResult, FromRow, FromRowError, FromValue,
    InMemoryKernel, Kernel, Query, QueryColumn, QueryDelete, QueryExpr, QueryExprValue, QueryFrom,
    QueryInsert, QueryResult, QuerySelect, QueryUpdate, QueryUpdateAssignment, Row, RowCodec,
    SqlTranslator, Statement, Uuid, Value, ValueType, decode, value,
};
