// mod date_and_time;
// mod numeric;

//TODO: CODE CAN BE SHARED (pretty muhch exactly the same)
use crate::connection::SqliteValue;
use crate::WasmSqlite;
use diesel::deserialize::{self, FromSql};
use diesel::query_builder::QueryId;
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types;
use diesel::sql_types::SqlType;

/// The returned pointer is *only* valid for the lifetime to the argument of
/// `from_sql`. This impl is intended for uses where you want to write a new
/// impl in terms of `String`, but don't want to allocate.
///
/// FIXME:
/// We have to return a
/// raw pointer instead of a reference with a lifetime due to the structure of
/// `FromSql` because we allocate a string in `read_text` (since SQLite memory is not shared with
/// us). So this function would
/// produce a  dangling pointer.
/*
// Not posible until we share mem with sqlite. There's no
// way to avoid an allocation into our host memory until then.

impl FromSql<sql_types::VarChar, WasmSqlite> for *const str {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        tracing::debug!("IN FROM SQL");
        let text = value.read_text();
        let text = text.as_str();
        Ok(text as *const _)
    }
}
*/

impl FromSql<sql_types::VarChar, WasmSqlite> for String {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(value.read_text())
    }
}

/* Not Possible until we share mem with SQLite
impl Queryable<sql_types::VarChar, WasmSqlite> for *const str {
    type Row = Self;

    fn build(row: Self::Row) -> deserialize::Result<Self> {
        Ok(row)
    }
}
*/

impl FromSql<sql_types::Binary, WasmSqlite> for Vec<u8> {
    fn from_sql(bytes: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(bytes.read_blob())
    }
}

/// The returned pointer is *only* valid for the lifetime to the argument of
/// `from_sql`. This impl is intended for uses where you want to write a new
/// impl in terms of `Vec<u8>`, but don't want to allocate. We have to return a
/// raw pointer instead of a reference with a lifetime due to the structure of
/// `FromSql`
/* Not possible until we share mem with SQLite
impl FromSql<sql_types::Binary, WasmSqlite> for *const [u8] {
    fn from_sql(bytes: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let bytes = bytes.read_blob();
        let bytes = bytes.as_slice();
        Ok(bytes as *const _)
    }
}

impl Queryable<sql_types::Binary, WasmSqlite> for *const [u8] {
    type Row = Self;

    fn build(row: Self::Row) -> deserialize::Result<Self> {
        Ok(row)
    }
}
*/

impl FromSql<sql_types::SmallInt, WasmSqlite> for i16 {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(value.read_integer() as i16)
    }
}

impl FromSql<sql_types::Integer, WasmSqlite> for i32 {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(value.read_integer())
    }
}

impl FromSql<sql_types::Bool, WasmSqlite> for bool {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(value.read_integer() != 0)
    }
}

impl FromSql<sql_types::BigInt, WasmSqlite> for i64 {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(value.read_long())
    }
}

impl FromSql<sql_types::Float, WasmSqlite> for f32 {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(value.read_double() as f32)
    }
}

impl FromSql<sql_types::Double, WasmSqlite> for f64 {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(value.read_double())
    }
}

impl ToSql<sql_types::Bool, WasmSqlite> for bool {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, WasmSqlite>) -> serialize::Result {
        let int_value = if *self { &1 } else { &0 };
        <i32 as ToSql<sql_types::Integer, WasmSqlite>>::to_sql(int_value, out)
    }
}

impl ToSql<sql_types::Text, WasmSqlite> for str {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, WasmSqlite>) -> serialize::Result {
        out.set_value(self);
        Ok(IsNull::No)
    }
}

impl ToSql<sql_types::Binary, WasmSqlite> for [u8] {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, WasmSqlite>) -> serialize::Result {
        out.set_value(self);
        Ok(IsNull::No)
    }
}

impl ToSql<sql_types::SmallInt, WasmSqlite> for i16 {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, WasmSqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl ToSql<sql_types::Integer, WasmSqlite> for i32 {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, WasmSqlite>) -> serialize::Result {
        out.set_value(*self);
        Ok(IsNull::No)
    }
}

impl ToSql<sql_types::BigInt, WasmSqlite> for i64 {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, WasmSqlite>) -> serialize::Result {
        out.set_value(*self);
        Ok(IsNull::No)
    }
}

impl ToSql<sql_types::Float, WasmSqlite> for f32 {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, WasmSqlite>) -> serialize::Result {
        out.set_value(*self as f64);
        Ok(IsNull::No)
    }
}

impl ToSql<sql_types::Double, WasmSqlite> for f64 {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, WasmSqlite>) -> serialize::Result {
        out.set_value(*self);
        Ok(IsNull::No)
    }
}

/// The SQLite timestamp with time zone type
///
/// ### [`ToSql`] impls
///
/// - [`chrono::NaiveDateTime`] with `feature = "chrono"`
/// - [`chrono::DateTime`] with `feature = "chrono"`
/// - [`time::PrimitiveDateTime`] with `feature = "time"`
/// - [`time::OffsetDateTime`] with `feature = "time"`
///
/// ### [`FromSql`] impls
///
/// - [`chrono::NaiveDateTime`] with `feature = "chrono"`
/// - [`chrono::DateTime`] with `feature = "chrono"`
/// - [`time::PrimitiveDateTime`] with `feature = "time"`
/// - [`time::OffsetDateTime`] with `feature = "time"`
///
/// [`ToSql`]: crate::serialize::ToSql
/// [`FromSql`]: crate::deserialize::FromSql
#[derive(Debug, Clone, Copy, Default, QueryId, SqlType)]
#[diesel(sqlite_type(name = "Text"))]
pub struct Timestamptz;
