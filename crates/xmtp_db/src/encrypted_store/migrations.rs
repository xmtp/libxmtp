use diesel::migration::{Migration, MigrationSource, MigrationVersion};
use diesel::{QueryResult, SqliteConnection};
use diesel_migrations::MigrationHarness;

use super::{ConnectionExt, MIGRATIONS, Sqlite, db_connection::DbConnection};
use crate::ConnectionError;

/// Trait for database migration operations.
///
/// WARNING: These operations are dangerous and can cause data loss.
/// They are intended for debugging and admin tools only.
pub trait QueryMigrations {
    /// Returns a list of all applied migration versions, most recent first.
    fn applied_migrations(&self) -> Result<Vec<String>, ConnectionError>;

    /// Returns a list of all available (embedded) migration names.
    fn available_migrations(&self) -> Result<Vec<String>, ConnectionError>;

    /// Rollback all migrations after and including the specified version.
    ///
    /// Refused on a database with data when it would revert the CBOR storage
    /// marker and keep some migrations, and so the data: every later open
    /// would reject the database as written before the marker
    /// (`StorageError::OldEncodingDatabase`). A rollback of every migration
    /// drops every table, so it runs; the next open creates a new database.
    ///
    /// The data check and the reverts run in one exclusive transaction, so no
    /// other connection writes between them. A revert that fails undoes the
    /// whole rollback and returns its error.
    ///
    /// WARNING: This is destructive and may cause data loss.
    fn rollback_to_version(&self, version: &str) -> Result<Vec<String>, ConnectionError>;

    /// Run a specific migration by name.
    ///
    /// NOTE: This runs the migration SQL directly without updating the
    /// schema_migrations tracking table.
    fn run_migration(&self, name: &str) -> Result<(), ConnectionError>;

    /// Revert a specific migration by name.
    ///
    /// NOTE: This runs the revert SQL directly without updating the
    /// schema_migrations tracking table.
    fn revert_migration(&self, name: &str) -> Result<(), ConnectionError>;

    /// Run all pending migrations.
    fn run_pending_migrations(&self) -> Result<Vec<String>, ConnectionError>;
}

fn get_migrations() -> Result<Vec<Box<dyn Migration<Sqlite>>>, ConnectionError> {
    MigrationSource::<Sqlite>::migrations(&MIGRATIONS)
        .map_err(|e| ConnectionError::Database(diesel::result::Error::QueryBuilderError(e)))
}

impl<C: ConnectionExt> QueryMigrations for DbConnection<C> {
    fn applied_migrations(&self) -> Result<Vec<String>, ConnectionError> {
        let applied: Vec<MigrationVersion<'static>> = self.raw_query(|conn| {
            conn.applied_migrations()
                .map_err(diesel::result::Error::QueryBuilderError)
        })?;
        Ok(applied.into_iter().map(|v| v.to_string()).collect())
    }

    fn available_migrations(&self) -> Result<Vec<String>, ConnectionError> {
        let migrations = get_migrations()?;
        let names: Vec<String> = migrations.iter().map(|m| m.name().to_string()).collect();
        Ok(names)
    }

    fn rollback_to_version(&self, version: &str) -> Result<Vec<String>, ConnectionError> {
        // Diesel orders versions as strings, not numbers, and embedded versions
        // do not all have the same length. Compare in the same order.
        let target: String = version.chars().filter(|c| c.is_ascii_digit()).collect();
        if target.is_empty() {
            return Err(ConnectionError::InvalidQuery(format!(
                "Invalid migration version: {version}"
            )));
        }

        // Versions compare as strings, as above: the rollback reverts the
        // marker when the target sorts at or before it, and every migration
        // when it sorts at or before the oldest applied one.
        let marker = super::CBOR_STORAGE_MIGRATION;
        let outcome = self.raw_query(|conn| {
            conn.exclusive_transaction(|conn| {
                let applied = |conn: &mut SqliteConnection| -> QueryResult<Vec<String>> {
                    Ok(conn
                        .applied_migrations()
                        .map_err(diesel::result::Error::QueryBuilderError)?
                        .into_iter()
                        .map(|version| version.to_string())
                        .collect())
                };
                let versions = applied(conn)?;
                let oldest = versions.iter().min().cloned().unwrap_or_default();
                let reverts_marker =
                    target.as_str() <= marker && versions.iter().any(|v| v == marker);
                let reverts_all = target.as_str() <= oldest.as_str();
                if reverts_marker && !reverts_all && super::holds_client_data(conn)? {
                    return Ok(Err(format!(
                        "Rolling back to {version} would revert the CBOR storage marker \
                         {marker} and keep this database's data, and every later open \
                         would reject it as written before the marker. Roll back to a \
                         version after {marker}, or roll back every migration (to \
                         {oldest} or earlier), which drops all data."
                    )));
                }

                let mut reverted = Vec::new();
                while let Some(current) = applied(conn)?.first().cloned() {
                    if current.as_str() < target.as_str() {
                        break;
                    }
                    let version = conn
                        .revert_last_migration(MIGRATIONS)
                        .map_err(diesel::result::Error::QueryBuilderError)?;
                    reverted.push(version.to_string());
                }
                Ok(Ok(reverted))
            })
        })?;
        outcome.map_err(ConnectionError::InvalidQuery)
    }

    fn run_migration(&self, name: &str) -> Result<(), ConnectionError> {
        let migrations = get_migrations()?;

        for migration in &migrations {
            if migration.name().to_string() == name {
                self.raw_query(|c| {
                    migration
                        .run(c)
                        .map_err(diesel::result::Error::QueryBuilderError)
                })?;
                return Ok(());
            }
        }

        Err(ConnectionError::InvalidQuery(format!(
            "Migration not found: {name}"
        )))
    }

    fn revert_migration(&self, name: &str) -> Result<(), ConnectionError> {
        let migrations = get_migrations()?;

        for migration in &migrations {
            if migration.name().to_string() == name {
                self.raw_query(|c| {
                    migration
                        .revert(c)
                        .map_err(diesel::result::Error::QueryBuilderError)
                })?;
                return Ok(());
            }
        }

        Err(ConnectionError::InvalidQuery(format!(
            "Migration not found: {name}"
        )))
    }

    fn run_pending_migrations(&self) -> Result<Vec<String>, ConnectionError> {
        let ran: Vec<String> = self.raw_query(|conn| {
            conn.run_pending_migrations(MIGRATIONS)
                .map(|versions| versions.into_iter().map(|v| v.to_string()).collect())
                .map_err(diesel::result::Error::QueryBuilderError)
        })?;
        Ok(ran)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::XmtpTestDb;

    const BASELINE: &str = "20260908000000";
    // 18 digits: string order puts it before the 14-digit version below.
    const RECEIVED_PROPOSALS: &str = "202609280000000000";
    const CONVERSATION_LIST_EXPIRY: &str = "20260928010000";

    /// Roll back to `target` on a new database and return what remains applied.
    async fn remaining_after_rollback(target: &str) -> Result<Vec<String>, ConnectionError> {
        let store = crate::TestDb::create_ephemeral_store().await;
        let conn = store.db();
        conn.rollback_to_version(target)?;
        conn.applied_migrations()
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn rollback_stops_at_a_shorter_version_in_diesel_order() {
        let remaining = remaining_after_rollback(CONVERSATION_LIST_EXPIRY).await?;
        assert_eq!(
            remaining.first().map(String::as_str),
            Some(RECEIVED_PROPOSALS)
        );
        assert!(remaining.iter().any(|version| version == BASELINE));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn rollback_reaches_a_longer_version_in_diesel_order() {
        let remaining = remaining_after_rollback(RECEIVED_PROPOSALS).await?;
        assert!(remaining.iter().all(
            |version| version != RECEIVED_PROPOSALS && version != CONVERSATION_LIST_EXPIRY
        ));
        assert!(remaining.iter().any(|version| version == BASELINE));
    }

    /// A rollback that would revert the CBOR marker on a database with data
    /// is refused: every later open would reject the database as written
    /// before the marker. Nothing is reverted, and the refusal names the
    /// marker and the target of a full rollback, the admin's only
    /// instructions. On an empty database it runs.
    #[xmtp_common::test(unwrap_try = true)]
    async fn rollback_keeps_the_cbor_marker_of_a_database_with_data() {
        use crate::encrypted_store::CBOR_STORAGE_MIGRATION;
        use diesel::connection::SimpleConnection;

        const BEFORE_MARKER: &str = "20261009020000";
        let store = crate::TestDb::create_ephemeral_store().await;
        let conn = store.db();
        conn.raw_query(|db| {
            db.batch_execute("INSERT INTO identity (inbox_id, installation_keys, credential_bytes) VALUES ('inbox', x'01', x'02')")
        })?;
        let before = conn.applied_migrations()?;
        for target in [BEFORE_MARKER, CBOR_STORAGE_MIGRATION] {
            let Err(ConnectionError::InvalidQuery(refusal)) = conn.rollback_to_version(target)
            else {
                panic!("the rollback to {target} was not refused");
            };
            assert!(
                refusal.contains(&format!("marker {CBOR_STORAGE_MIGRATION} ")),
                "{refusal}"
            );
            assert!(
                refusal.contains(&format!("(to {BASELINE} or earlier)")),
                "{refusal}"
            );
        }
        assert_eq!(conn.applied_migrations()?, before);

        conn.raw_query(|db| db.batch_execute("DELETE FROM identity"))?;
        conn.rollback_to_version(BEFORE_MARKER)?;
        assert!(
            !conn
                .applied_migrations()?
                .iter()
                .any(|version| version == CBOR_STORAGE_MIGRATION)
        );
    }

    /// The data check and the reverts are one exclusive transaction: when a
    /// revert fails, the whole rollback is undone, the marker's revert
    /// included, and the error is returned.
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_failed_revert_undoes_the_whole_rollback() {
        use diesel::connection::SimpleConnection;

        let store = crate::TestDb::create_ephemeral_store().await;
        let conn = store.db();
        // The down migration that runs after the marker's drops this index.
        conn.raw_query(|db| db.batch_execute("DROP INDEX group_messages_group_reference"))?;
        let before = conn.applied_migrations()?;
        assert!(conn.rollback_to_version(BASELINE).is_err());
        assert_eq!(conn.applied_migrations()?, before);
    }

    /// A rollback of every migration on a database with data runs: it drops
    /// every table, so the next open creates a new database, with no data.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_full_rollback_of_a_database_with_data_runs() {
        use crate::{EncryptedMessageStore, TestDb};
        use diesel::connection::SimpleConnection;

        let path = xmtp_common::tmp_path();
        {
            let store =
                EncryptedMessageStore::new(TestDb::create_database(Some(path.clone())).await)?;
            let conn = store.db();
            conn.raw_query(|db| {
                db.batch_execute("INSERT INTO identity (inbox_id, installation_keys, credential_bytes) VALUES ('inbox', x'01', x'02')")
            })?;
            assert!(!conn.rollback_to_version(BASELINE)?.is_empty());
            assert!(conn.applied_migrations()?.is_empty());
        }
        let store = EncryptedMessageStore::new(TestDb::create_database(Some(path.clone())).await)?;
        assert!(!store.db().raw_query(super::super::holds_client_data)?);
        assert!(
            store
                .db()
                .applied_migrations()?
                .iter()
                .any(|version| version == crate::encrypted_store::CBOR_STORAGE_MIGRATION)
        );
        drop(store);
        EncryptedMessageStore::<()>::remove_db_files(path);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "migrations/tests.rs"]
mod upgrade_tests;

#[cfg(test)]
#[path = "migrations/storage_format_tests.rs"]
mod storage_format_tests;
