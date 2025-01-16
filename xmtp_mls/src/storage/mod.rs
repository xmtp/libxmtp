pub(super) mod encrypted_store;
mod errors;
pub mod serialization;
pub mod sql_key_store;
pub mod xmtp_openmls_provider;

pub use encrypted_store::*;
pub use errors::*;

/// Initialize the SQLite WebAssembly Library
#[cfg(target_arch = "wasm32")]
pub async fn init_sqlite() {
    sqlite_web::init_sqlite().await;
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
                let _ = self.raw_query(true, |conn| query.execute(conn)).unwrap();
            }
        }

        pub fn intents_published(&self) -> i32 {
            self.raw_query(false, |conn| {
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
            self.raw_query(false, |conn| {
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
            self.raw_query(false, |conn| {
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
