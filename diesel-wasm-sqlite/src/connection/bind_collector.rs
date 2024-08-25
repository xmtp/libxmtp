use crate::{SqliteType, WasmSqlite};
use diesel::{
    query_builder::{BindCollector, MoveableBindCollector},
    result::QueryResult,
    serialize::{IsNull, Output},
    sql_types::HasSqlType,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default)]
pub struct SqliteBindCollector<'a> {
    pub(crate) binds: Vec<(InternalSqliteBindValue<'a>, SqliteType)>,
}

impl SqliteBindCollector<'_> {
    pub(crate) fn new() -> Self {
        Self { binds: Vec::new() }
    }
}

/// This type represents a value bound to
/// a sqlite prepared statement
///
/// It can be constructed via the various `From<T>` implementations
#[derive(Debug)]
pub struct SqliteBindValue<'a> {
    pub(crate) inner: InternalSqliteBindValue<'a>,
}

impl<'a> From<i32> for SqliteBindValue<'a> {
    fn from(i: i32) -> Self {
        Self {
            inner: InternalSqliteBindValue::I32(i),
        }
    }
}

impl<'a> From<i64> for SqliteBindValue<'a> {
    fn from(i: i64) -> Self {
        Self {
            inner: InternalSqliteBindValue::I64(i),
        }
    }
}

impl<'a> From<f64> for SqliteBindValue<'a> {
    fn from(f: f64) -> Self {
        Self {
            inner: InternalSqliteBindValue::F64(f),
        }
    }
}

impl<'a, T> From<Option<T>> for SqliteBindValue<'a>
where
    T: Into<SqliteBindValue<'a>>,
{
    fn from(o: Option<T>) -> Self {
        match o {
            Some(v) => v.into(),
            None => Self {
                inner: InternalSqliteBindValue::Null,
            },
        }
    }
}

impl<'a> From<&'a str> for SqliteBindValue<'a> {
    fn from(s: &'a str) -> Self {
        Self {
            inner: InternalSqliteBindValue::BorrowedString(s),
        }
    }
}

impl<'a> From<String> for SqliteBindValue<'a> {
    fn from(s: String) -> Self {
        Self {
            inner: InternalSqliteBindValue::String(s),
        }
    }
}

impl<'a> From<Vec<u8>> for SqliteBindValue<'a> {
    fn from(b: Vec<u8>) -> Self {
        Self {
            inner: InternalSqliteBindValue::Binary(b),
        }
    }
}

impl<'a> From<&'a [u8]> for SqliteBindValue<'a> {
    fn from(b: &'a [u8]) -> Self {
        Self {
            inner: InternalSqliteBindValue::BorrowedBinary(b),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum InternalSqliteBindValue<'a> {
    BorrowedString(&'a str),
    String(String),
    BorrowedBinary(&'a [u8]),
    Binary(Vec<u8>),
    I32(i32),
    I64(i64),
    F64(f64),
    Null,
}

impl std::fmt::Display for InternalSqliteBindValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = match self {
            InternalSqliteBindValue::BorrowedString(_) | InternalSqliteBindValue::String(_) => {
                "Text"
            }
            InternalSqliteBindValue::BorrowedBinary(_) | InternalSqliteBindValue::Binary(_) => {
                "Binary"
            }
            InternalSqliteBindValue::I32(_) | InternalSqliteBindValue::I64(_) => "Integer",
            InternalSqliteBindValue::F64(_) => "Float",
            InternalSqliteBindValue::Null => "Null",
        };
        f.write_str(n)
    }
}
/*
impl InternalSqliteBindValue<'_> {
    #[allow(unsafe_code)] // ffi function calls
    pub(crate) fn result_of(self, ctx: &mut i32) {
        let sqlite3 = crate::get_sqlite_unchecked();
        match self {
            InternalSqliteBindValue::BorrowedString(s) => sqlite3.result_text(*ctx, s.to_string()),
            InternalSqliteBindValue::String(s) => sqlite3.result_text(*ctx, s.to_string()),
            InternalSqliteBindValue::Binary(b) => sqlite3.result_blob(*ctx, b.to_vec()),
            InternalSqliteBindValue::BorrowedBinary(b) => sqlite3.result_blob(*ctx, b.to_vec()),
            InternalSqliteBindValue::I32(i) => sqlite3.result_int(*ctx, i),
            InternalSqliteBindValue::I64(l) => sqlite3.result_int64(*ctx, l),
            InternalSqliteBindValue::F64(d) => sqlite3.result_double(*ctx, d),
            InternalSqliteBindValue::Null => sqlite3.result_null(*ctx),
        }
    }
}
*/

impl<'a> BindCollector<'a, WasmSqlite> for SqliteBindCollector<'a> {
    type Buffer = SqliteBindValue<'a>;

    fn push_bound_value<T, U>(&mut self, bind: &'a U, metadata_lookup: &mut ()) -> QueryResult<()>
    where
        WasmSqlite: diesel::sql_types::HasSqlType<T>,
        U: diesel::serialize::ToSql<T, WasmSqlite> + ?Sized,
    {
        let value = SqliteBindValue {
            inner: InternalSqliteBindValue::Null,
        };
        let mut to_sql_output = Output::new(value, metadata_lookup);
        let is_null = bind
            .to_sql(&mut to_sql_output)
            .map_err(diesel::result::Error::SerializationError)?;
        let bind = to_sql_output.into_inner();
        let metadata = WasmSqlite::metadata(metadata_lookup);

        self.binds.push((
            match is_null {
                IsNull::No => bind.inner,
                IsNull::Yes => InternalSqliteBindValue::Null,
            },
            metadata,
        ));
        Ok(())
    }

    fn push_null_value(&mut self, metadata: SqliteType) -> QueryResult<()> {
        self.binds.push((InternalSqliteBindValue::Null, metadata));
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OwnedSqliteBindValue {
    String(String),
    Binary(Vec<u8>),
    I32(i32),
    I64(i64),
    F64(f64),
    Null,
}

impl<'a> std::convert::From<&InternalSqliteBindValue<'a>> for OwnedSqliteBindValue {
    fn from(value: &InternalSqliteBindValue<'a>) -> Self {
        match value {
            InternalSqliteBindValue::String(s) => Self::String(s.clone()),
            InternalSqliteBindValue::BorrowedString(s) => Self::String(String::from(*s)),
            InternalSqliteBindValue::Binary(b) => Self::Binary(b.clone()),
            InternalSqliteBindValue::BorrowedBinary(s) => Self::Binary(Vec::from(*s)),
            InternalSqliteBindValue::I32(val) => Self::I32(*val),
            InternalSqliteBindValue::I64(val) => Self::I64(*val),
            InternalSqliteBindValue::F64(val) => Self::F64(*val),
            InternalSqliteBindValue::Null => Self::Null,
        }
    }
}

impl<'a> std::convert::From<&OwnedSqliteBindValue> for InternalSqliteBindValue<'a> {
    fn from(value: &OwnedSqliteBindValue) -> Self {
        match value {
            OwnedSqliteBindValue::String(s) => Self::String(s.clone()),
            OwnedSqliteBindValue::Binary(b) => Self::Binary(b.clone()),
            OwnedSqliteBindValue::I32(val) => Self::I32(*val),
            OwnedSqliteBindValue::I64(val) => Self::I64(*val),
            OwnedSqliteBindValue::F64(val) => Self::F64(*val),
            OwnedSqliteBindValue::Null => Self::Null,
        }
    }
}

#[derive(Debug)]
/// Sqlite bind collector data that is movable across threads
pub struct SqliteBindCollectorData {
    pub binds: Vec<(OwnedSqliteBindValue, SqliteType)>,
}

impl MoveableBindCollector<WasmSqlite> for SqliteBindCollector<'_> {
    type BindData = SqliteBindCollectorData;

    fn moveable(&self) -> Self::BindData {
        let mut binds = Vec::with_capacity(self.binds.len());
        for b in self
            .binds
            .iter()
            .map(|(bind, tpe)| (OwnedSqliteBindValue::from(bind), *tpe))
        {
            binds.push(b);
        }
        SqliteBindCollectorData { binds }
    }

    fn append_bind_data(&mut self, from: &Self::BindData) {
        self.binds.reserve_exact(from.binds.len());
        self.binds.extend(
            from.binds
                .iter()
                .map(|(bind, tpe)| (InternalSqliteBindValue::from(bind), *tpe)),
        );
    }
}
