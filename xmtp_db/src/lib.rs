#![warn(clippy::unwrap_used)]

mod configuration;
pub mod encrypted_store;
mod errors;
pub mod serialization;
pub use serialization::*;
pub mod sql_key_store;
mod traits;
pub use traits::*;
pub mod xmtp_openmls_provider;
pub use xmtp_openmls_provider::*;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
#[cfg(any(test, feature = "test-utils"))]
pub use test_utils::*;

pub use diesel;
use diesel::connection::SimpleConnection;
pub use encrypted_store::*;
pub use errors::*;

/// The default platform-specific store
pub type DefaultStore = EncryptedMessageStore<database::DefaultDatabase>;

impl<C: ConnectionExt> DbConnection<C> {
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

#[cfg(not(target_arch = "wasm32"))]
pub async fn init_sqlite() {}

#[cfg(any(test, feature = "test-utils"))]
pub mod test_util {
    #![allow(clippy::unwrap_used)]

    #[cfg_attr(not(target_arch = "wasm32"), ctor::ctor)]
    #[cfg(not(target_arch = "wasm32"))]
    fn _setup() {
        xmtp_common::logger();
    }

    use super::*;
    use diesel::{RunQueryDsl, connection::LoadConnection, deserialize::FromSqlRow, sql_query};
    impl<C: ConnectionExt> DbConnection<C> {
        /// Create a new table and register triggers for tracking column updates
        pub fn register_triggers(&self) {
            tracing::info!("Registering triggers");
            let queries = vec![
                r#"
                CREATE TABLE test_metadata (
                    intents_created INT DEFAULT 0,
                    intents_published INT DEFAULT 0,
                    intents_deleted INT DEFAULT 0,
                    intents_processed INT DEFAULT 0,
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
                r#"CREATE TRIGGER intents_processed_tracking AFTER UPDATE OF state ON group_intents
                FOR EACH ROW
                WHEN NEW.state = 5
                BEGIN
                    UPDATE test_metadata SET intents_processed = intents_processed + 1;
                END;"#,
                r#"CREATE TRIGGER intents_deleted_tracking AFTER DELETE ON group_intents
                FOR EACH ROW
                BEGIN
                    UPDATE test_metadata SET intents_deleted = intents_deleted + 1;
                END;"#,
                r#"INSERT INTO test_metadata (
                    intents_created,
                    intents_deleted,
                    intents_published,
                    intents_processed
                ) VALUES (0, 0, 0, 0);"#,
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
                Ok(
                    <i32 as FromSqlRow<diesel::sql_types::Integer, _>>::build_from_row(&row)
                        .unwrap(),
                )
            })
            .unwrap()
        }

        pub fn intents_processed(&self) -> i32 {
            self.raw_query_read(|conn| {
                let mut row = conn
                    .load(sql_query(
                        "SELECT intents_processed FROM test_metadata WHERE rowid = 1",
                    ))
                    .unwrap();
                let row = row.next().unwrap().unwrap();
                Ok(
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
                Ok(
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
                Ok(
                    <i32 as FromSqlRow<diesel::sql_types::Integer, _>>::build_from_row(&row)
                        .unwrap(),
                )
            })
            .unwrap()
        }
    }
}
