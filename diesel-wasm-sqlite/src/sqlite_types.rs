use super::backend::{SqliteType, WasmSqlite};
use diesel::sql_types::*;

pub mod to_sql;

//TODO These Database Types are defined in the wasm file and should be imported.
// this is easier for now because of quirks with converting from JsValue to integer within extern
// "C" declaration.
macro_rules! impl_has_sql_type {
    ($type:ty, $sql_type:expr) => {
        impl HasSqlType<$type> for WasmSqlite {
            fn metadata(_: &mut ()) -> SqliteType {
                $sql_type
            }
        }
    };
}

impl_has_sql_type!(Bool, SqliteType::Integer);
impl_has_sql_type!(SmallInt, SqliteType::SmallInt);
impl_has_sql_type!(Integer, SqliteType::Integer);
impl_has_sql_type!(BigInt, SqliteType::Long);
impl_has_sql_type!(Float, SqliteType::Float);
impl_has_sql_type!(Double, SqliteType::Double);
impl_has_sql_type!(Numeric, SqliteType::Double);
impl_has_sql_type!(Text, SqliteType::Text);
impl_has_sql_type!(Binary, SqliteType::Binary);
impl_has_sql_type!(Date, SqliteType::Text);
impl_has_sql_type!(Time, SqliteType::Text);
impl_has_sql_type!(Timestamp, SqliteType::Text);
