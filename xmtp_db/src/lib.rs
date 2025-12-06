#![warn(clippy::unwrap_used)]

pub mod encrypted_store;
mod errors;
pub mod serialization;
pub use serialization::*;
pub mod sql_key_store;
mod traits;
pub use traits::*;
pub mod xmtp_openmls_provider;
pub use xmtp_openmls_provider::*;
#[cfg(any(feature = "test-utils", test))]
pub mod mock;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
#[cfg(any(test, feature = "test-utils"))]
pub use test_utils::*;

pub use diesel;
pub use encrypted_store::*;
pub use errors::*;
pub use xmtp_proto as proto;

use diesel::connection::SimpleConnection;

use crate::sql_key_store::SqlKeyStore;

/// The default platform-specific store
pub type DefaultStore = EncryptedMessageStore<database::DefaultDatabase>;
pub type DefaultDbConnection = <DefaultStore as XmtpDb>::DbQuery;
pub type DefaultMlsStore = SqlKeyStore<<DefaultStore as XmtpDb>::DbQuery>;

pub mod prelude {
    pub use super::ReadOnly;
    pub use super::association_state::QueryAssociationStateCache;
    pub use super::consent_record::QueryConsentRecord;
    pub use super::conversation_list::QueryConversationList;
    pub use super::group::QueryDms;
    pub use super::group::QueryGroup;
    pub use super::group::QueryGroupVersion;
    pub use super::group_intent::QueryGroupIntent;
    pub use super::group_message::QueryGroupMessage;
    pub use super::identity::QueryIdentity;
    pub use super::identity_cache::QueryIdentityCache;
    pub use super::identity_update::QueryIdentityUpdates;
    pub use super::key_package_history::QueryKeyPackageHistory;
    pub use super::key_store_entry::QueryKeyStoreEntry;
    pub use super::local_commit_log::QueryLocalCommitLog;
    pub use super::pragmas::Pragmas;
    pub use super::processed_device_sync_messages::QueryDeviceSyncMessages;
    pub use super::readd_status::QueryReaddStatus;
    pub use super::refresh_state::QueryRefreshState;
    pub use super::remote_commit_log::QueryRemoteCommitLog;
    pub use super::tasks::QueryTasks;
    pub use super::traits::*;
}

pub trait ReadOnly {
    #[allow(unused)]
    fn enable_readonly(&self) -> Result<(), StorageError>;

    #[allow(unused)]
    fn disable_readonly(&self) -> Result<(), StorageError>;
}

impl<C: ConnectionExt> ReadOnly for DbConnection<C> {
    #[allow(unused)]
    fn enable_readonly(&self) -> Result<(), StorageError> {
        self.raw_query_write(|conn| conn.batch_execute("PRAGMA query_only = ON;"))?;
        Ok(())
    }

    #[allow(unused)]
    fn disable_readonly(&self) -> Result<(), StorageError> {
        self.raw_query_write(|conn| conn.batch_execute("PRAGMA query_only = OFF;"))?;
        Ok(())
    }
}

impl<T> ReadOnly for &T
where
    T: ReadOnly,
{
    #[allow(unused)]
    fn enable_readonly(&self) -> Result<(), StorageError> {
        (**self).enable_readonly()
    }

    #[allow(unused)]
    fn disable_readonly(&self) -> Result<(), StorageError> {
        (**self).disable_readonly()
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn init_sqlite() {
    // This is a no-op for wasm32
}
#[cfg_attr(not(target_arch = "wasm32"), ctor::ctor)]
#[cfg(all(test, not(target_arch = "wasm32")))]
fn test_setup() {
    xmtp_common::logger();
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn init_sqlite() {}

#[cfg(any(test, feature = "test-utils"))]
pub mod test_util {
    #![allow(clippy::unwrap_used)]

    use crate::group_message::{GroupMessageKind, StoredGroupMessage};

    use super::*;
    use diesel::{
        ExpressionMethods, RunQueryDsl, connection::LoadConnection, deserialize::FromSqlRow,
        sql_query,
    };
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
                r#"
                -- Create a table to store history of deleted intent payload hashes
                CREATE TABLE deleted_intents_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    intent_id INTEGER NOT NULL,
                    payload_hash BLOB,
                    deleted_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                );
                "#,
                r#"
                -- Create a table to store history of key package rotation timestamps
                CREATE TABLE key_package_rotation_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    next_key_package_rotation_ns BIGINT,
                    updated_at BIGINT NOT NULL
                );
                "#,
                r#"
                -- Modify the deletion trigger to record payload hash history
                CREATE TRIGGER intents_deleted_tracking AFTER DELETE ON group_intents
                FOR EACH ROW
                BEGIN
                    -- Update the counter in test_metadata
                    UPDATE test_metadata SET intents_deleted = intents_deleted + 1;
                    -- Insert the deleted intent's information into history table
                    INSERT INTO deleted_intents_history (intent_id, payload_hash)
                    VALUES (OLD.id, OLD.payload_hash);
                END;
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
                r#"
                CREATE TRIGGER track_key_package_rotation AFTER UPDATE OF next_key_package_rotation_ns ON identity
                FOR EACH ROW
                WHEN OLD.next_key_package_rotation_ns IS NOT NEW.next_key_package_rotation_ns
                BEGIN
                    INSERT INTO key_package_rotation_history (next_key_package_rotation_ns, updated_at)
                    VALUES (NEW.next_key_package_rotation_ns, (strftime('%s', 'now') || substr(strftime('%f', 'now'), 4)) * 1);
                END;
                "#,
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

        pub fn intent_payloads_deleted(&self) -> Vec<Vec<u8>> {
            let mut hashes = vec![];
            self.raw_query_read(|conn| {
                let row = conn
                    .load(sql_query(
                        "SELECT payload_hash FROM deleted_intents_history",
                    ))
                    .unwrap();
                for r in row {
                    hashes.push(
                        <Vec<u8> as FromSqlRow<diesel::sql_types::Binary, _>>::build_from_row(
                            &r.unwrap(),
                        )
                        .unwrap(),
                    );
                }
                Ok(())
            })
            .unwrap();
            hashes
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

        pub fn missing_messages(&self, sequence_ids: &[u64]) -> Vec<StoredGroupMessage> {
            use crate::schema::group_messages::{self, dsl};
            use diesel::QueryDsl;
            let sequence_ids: Vec<i64> = sequence_ids.iter().copied().map(|id| id as i64).collect();
            let query = dsl::group_messages
                .filter(dsl::sequence_id.is_not_null())
                .filter(group_messages::sequence_id.ne_all(sequence_ids))
                .filter(group_messages::kind.eq(GroupMessageKind::Application))
                .order(group_messages::sequence_id.asc());

            self.raw_query_read(|conn| query.load(conn)).unwrap()
        }

        pub fn key_package_rotation_history(&self) -> Vec<(i64, i64)> {
            let mut history = vec![];
            self.raw_query_read(|conn| {
                let rows = conn
                    .load(sql_query(
                        "SELECT next_key_package_rotation_ns, updated_at FROM key_package_rotation_history ORDER BY id ASC",
                    ))
                    .unwrap();
                for row in rows {
                    let row = row.unwrap();
                    let rotation_ns = <i64 as FromSqlRow<diesel::sql_types::BigInt, _>>::build_from_row(&row)
                        .unwrap();
                    let updated_at = <i64 as FromSqlRow<diesel::sql_types::BigInt, _>>::build_from_row(&row)
                        .unwrap();
                    history.push((rotation_ns, updated_at));
                }
                Ok(())
            })
            .unwrap();
            history
        }
    }
}
