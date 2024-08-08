use super::backend::{SqliteType, WasmSqlite};
use bitflags::bitflags;
use diesel::sql_types::*;
use serde::{Deserialize, Serialize};

//TODO These Database Types are defined in the wasm file and should be imported.
// this is easier for now because of quirks with converting from JsValue to integer within extern
// "C" declaration.

// result codes
pub const SQLITE_DONE: i32 = 101;
pub const SQLITE_ROW: i32 = 100;

// Fundamental datatypes.
// https://www.sqlite.org/c3ref/c_blob.html
pub const SQLITE_INTEGER: i32 = 1;
pub const SQLITE_FLOAT: i32 = 2;
pub const SQLITE_TEXT: i32 = 3;
pub const SQLITE_BLOB: i32 = 4;
pub const SQLITE_NULL: i32 = 5;

/// `SqlitePrepareOptions` imported type
#[derive(Serialize, Deserialize, Default, Clone, Debug, Copy)]
pub struct PrepareOptions {
    pub flags: Option<i32>,
    pub unscoped: Option<i32>,
}

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

bitflags! {
    pub struct SqliteOpenFlags: u32 {
        const SQLITE_OPEN_READONLY       = 0x00000001; /* Ok for sqlite3_open_v2() */
        const SQLITE_OPEN_READWRITE      = 0x00000002; /* Ok for sqlite3_open_v2() */
        const SQLITE_OPEN_CREATE         = 0x00000004; /* Ok for sqlite3_open_v2() */
        const SQLITE_OPEN_DELETEONCLOSE  = 0x00000008; /* VFS only */
        const SQLITE_OPEN_EXCLUSIVE      = 0x00000010; /* VFS only */
        const SQLITE_OPEN_AUTOPROXY      = 0x00000020; /* VFS only */
        const SQLITE_OPEN_URI            = 0x00000040; /* Ok for sqlite3_open_v2() */
        const SQLITE_OPEN_MEMORY         = 0x00000080; /* Ok for sqlite3_open_v2() */
        const SQLITE_OPEN_MAIN_DB        = 0x00000100; /* VFS only */
        const SQLITE_OPEN_TEMP_DB        = 0x00000200; /* VFS only */
        const SQLITE_OPEN_TRANSIENT_DB   = 0x00000400; /* VFS only */
        const SQLITE_OPEN_MAIN_JOURNAL   = 0x00000800; /* VFS only */
        const SQLITE_OPEN_TEMP_JOURNAL   = 0x00001000; /* VFS only */
        const SQLITE_OPEN_SUBJOURNAL     = 0x00002000; /* VFS only */
        const SQLITE_OPEN_SUPER_JOURNAL  = 0x00004000; /* VFS only */
        const SQLITE_OPEN_NOMUTEX        = 0x00008000; /* Ok for sqlite3_open_v2() */
        const SQLITE_OPEN_FULLMUTEX      = 0x00010000; /* Ok for sqlite3_open_v2() */
        const SQLITE_OPEN_SHAREDCACHE    = 0x00020000; /* Ok for sqlite3_open_v2() */
        const SQLITE_OPEN_PRIVATECACHE   = 0x00040000; /* Ok for sqlite3_open_v2() */
        const SQLITE_OPEN_WAL            = 0x00080000; /* VFS only */
        const SQLITE_OPEN_NOFOLLOW       = 0x01000000; /* Ok for sqlite3_open_v2() */
        const SQLITE_OPEN_EXRESCODE      = 0x02000000; /* Extended result codes */
    }
}

// SQLite Text Encodings https://www.sqlite.org/capi3ref.html#SQLITE_ANY
bitflags! {
    pub struct SqliteFlags: u32 {
        const SQLITE_UTF8          = 1;   /* IMP: R-37514-35566 */
        const SQLITE_UTF16LE       = 2;   /* IMP: R-03371-37637 */
        const SQLITE_UTF16BE       = 3;   /* IMP: R-51971-34154 */
        const SQLITE_UTF16         = 4;   /* Use native byte order */
        const SQLITE_ANY           = 5;   /* Deprecated */
        const SQLITE_UTF16_ALIGNED = 8;   /* sqlite3_create_collation only */

        /// SQLite Function Flags https://www.sqlite.org/capi3ref.html#sqlitedeterministic
        const SQLITE_DETERMINISTIC  = 0x000000800;
        const SQLITE_DIRECTONLY     = 0x000080000;
        const SQLITE_SUBTYPE        = 0x000100000;
        const SQLITE_INNOCUOUS      = 0x000200000;
        const SQLITE_RESULT_SUBTYPE = 0x001000000;
    }
}

// SQLite Prepare Flags https://www.sqlite.org/c3ref/c_prepare_normalize.html#sqlitepreparepersistent
bitflags! {
    pub struct SqlitePrepareFlags: i32 {
        const SQLITE_PREPARE_PERSISTENT = 0x01;
        const SQLITE_PREPARE_NORMALIZE  = 0x02;
        const SQLITE_PREPARE_NO_VTAB    = 0x04;
    }
}
