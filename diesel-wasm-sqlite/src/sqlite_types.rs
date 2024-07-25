use super::backend::{SqliteType, WasmSqlite};
use bitflags::bitflags;
use diesel::sql_types::*;

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
