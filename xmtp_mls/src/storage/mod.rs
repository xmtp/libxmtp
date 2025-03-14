pub(super) mod encrypted_store;
mod errors;
pub mod serialization;
pub mod sql_key_store;
pub mod xmtp_openmls_provider;

use diesel::connection::SimpleConnection;
pub use encrypted_store::*;
pub use errors::*;
impl DbConnection {
    #[allow(unused)]
    pub(crate) fn enable_readonly(&self) -> Result<(), StorageError> {
        self.raw_query_write(|conn| conn.batch_execute("PRAGMA query_only = ON;"))?;
        Ok(())
    }

    #[allow(unused)]
    pub(crate) fn disable_readonly(&self) -> Result<(), StorageError> {
        self.raw_query_write(|conn| conn.batch_execute("PRAGMA query_only = OFF;"))?;
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_export::*;

#[cfg(target_arch = "wasm32")]
mod wasm_export {
    pub static SQLITE: tokio::sync::OnceCell<Result<OpfsSAHPoolUtil, String>> =
        tokio::sync::OnceCell::const_new();
    pub use sqlite_wasm_rs::export::{OpfsSAHError, OpfsSAHPoolUtil};

    /// Initialize the SQLite WebAssembly Library
    pub async fn init_sqlite() {
        use sqlite_wasm_rs::export::OpfsSAHPoolCfg;
        SQLITE
            .get_or_init(|| async {
                let cfg = OpfsSAHPoolCfg {
                    vfs_name: "opfs-libxmtp".to_string(),
                    directory: ".opfs-libxmtp-metadata".to_string(),
                    clear_on_init: false,
                    initial_capacity: 6,
                };
                let r = sqlite_wasm_rs::export::install_opfs_sahpool(Some(&cfg), true).await;
                if let Err(ref e) = r {
                    tracing::warn!("Encountered possible vfs error {e}");
                }
                // the error is not send or sync as required by tokio OnceCell
                r.map_err(|e| format!("{e}"))
            })
            .await;
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn init_sqlite() {}

#[cfg(any(test, feature = "test-utils"))]
pub mod test_util {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use diesel::{connection::LoadConnection, deserialize::FromSqlRow, sql_query, RunQueryDsl};
    impl DbConnection {
        /// Create a new table and register triggers for tracking column updates
        pub fn register_triggers(&self) {
            tracing::info!("Registering triggers");
            let queries = vec![
                r#"
                CREATE TABLE test_metadata (
                    intents_created INT DEFAULT 0,
                    intents_published INT DEFAULT 0,
                    intents_deleted INT DEFAULT 0,
                    rowid integer PRIMARY KEY CHECK (rowid = 1) -- There can only be one meta
                );
                "#,
                r#"CREATE TRIGGER intents_created_tracking AFTER INSERT on group_intents
                BEGIN
                    UPDATE test_metadata SET intents_created = intents_created + 1;
                END;"#,
                r#"CREATE TRIGGER intents_published_tracking AFTER UPDATE OF state ON group_intents
                FOR EACH ROW
                WHEN NEW.state = 2 AND OLD.state !=2
                BEGIN
                    UPDATE test_metadata SET intents_published = intents_published + 1;
                END;"#,
                r#"CREATE TRIGGER intents_deleted_tracking AFTER DELETE ON group_intents
                FOR EACH ROW
                BEGIN
                    UPDATE test_metadata SET intents_deleted = intents_deleted + 1;
                END;"#,
                r#"INSERT INTO test_metadata (
                    intents_created,
                    intents_deleted,
                    intents_published
                ) VALUES (0, 0,0);"#,
            ];

            for query in queries {
                let query = diesel::sql_query(query);
                let _ = self.raw_query_write(|conn| query.execute(conn)).unwrap();
            }
        }

        /// Disable sqlcipher memory security
        pub fn disable_memory_security(&self) {
            let query = r#"PRAGMA cipher_memory_security = OFF"#;
            let query = diesel::sql_query(query);
            let _ = self.raw_query_read(|c| query.clone().execute(c)).unwrap();
            let _ = self.raw_query_write(|c| query.execute(c)).unwrap();
        }

        pub fn intents_published(&self) -> i32 {
            self.raw_query_read(|conn| {
                let mut row = conn
                    .load(sql_query(
                        "SELECT intents_published FROM test_metadata WHERE rowid = 1",
                    ))
                    .unwrap();
                let row = row.next().unwrap().unwrap();
                Ok::<_, StorageError>(
                    <i32 as FromSqlRow<diesel::sql_types::Integer, _>>::build_from_row(&row)
                        .unwrap(),
                )
            })
            .unwrap()
        }

        pub fn intents_deleted(&self) -> i32 {
            self.raw_query_read(|conn| {
                let mut row = conn
                    .load(sql_query("SELECT intents_deleted FROM test_metadata"))
                    .unwrap();
                let row = row.next().unwrap().unwrap();
                Ok::<_, StorageError>(
                    <i32 as FromSqlRow<diesel::sql_types::Integer, _>>::build_from_row(&row)
                        .unwrap(),
                )
            })
            .unwrap()
        }

        pub fn intents_created(&self) -> i32 {
            self.raw_query_read(|conn| {
                let mut row = conn
                    .load(sql_query("SELECT intents_created FROM test_metadata"))
                    .unwrap();
                let row = row.next().unwrap().unwrap();
                Ok::<_, StorageError>(
                    <i32 as FromSqlRow<diesel::sql_types::Integer, _>>::build_from_row(&row)
                        .unwrap(),
                )
            })
            .unwrap()
        }
    }
}
