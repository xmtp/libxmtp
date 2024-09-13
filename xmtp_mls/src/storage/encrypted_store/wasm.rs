use std::sync::Arc;

use diesel::{
    connection::{AnsiTransactionManager, TransactionManager},
    prelude::*,
    r2d2::{CustomizeConnection, Error as R2Error, PoolTransactionManager},
    result::{DatabaseErrorKind, Error},
    sql_query,
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

#[cfg(not(target_arch = "wasm32"))]
pub use diesel::sqlite::Sqlite;
#[cfg(target_arch = "wasm32")]
pub use diesel_wasm_sqlite::WasmSqlite as Sqlite;

#[cfg(not(target_arch = "wasm32"))]
pub use diesel::sqlite::SqliteConnection;
#[cfg(target_arch = "wasm32")]
pub use diesel_wasm_sqlite::connection::WasmSqliteConnection as SqliteConnection;

use self::db_connection::DbConnection;

use super::StorageError;
use crate::{xmtp_openmls_provider::XmtpOpenMlsProvider, Store};

#[cfg(target_arch = "wasm32")]
pub static SQL_CONN: std::cell::OnceCell<SqliteConnection> = std::cell::OnceCell::new();
