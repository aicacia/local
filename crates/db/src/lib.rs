#[cfg(feature = "native")]
mod native;

#[cfg(feature = "replica")]
pub use converge::{
    AutomergeRowCodec, Engine, EngineError, EngineResult, FromRow, FromRowError, FromValue,
    InMemoryKernel, Kernel, Query, QueryColumn, QueryDelete, QueryExpr, QueryExprValue, QueryFrom,
    QueryInsert, QueryResult, QuerySelect, QueryUpdate, QueryUpdateAssignment, Row, RowCodec,
    SqlTranslator, Statement, Uuid, Value, ValueType, decode, value,
};
#[cfg(feature = "native")]
pub use native::{NativeEngine, open_native_engine};
