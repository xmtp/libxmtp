use diesel::migration::{Migration, MigrationSource, MigrationVersion};
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

        let mut reverted = Vec::new();

        loop {
            let applied = self.applied_migrations()?;
            let Some(current_version) = applied.first() else {
                break;
            };

            if current_version.as_str() < target.as_str() {
                break;
            }

            let result = self.raw_query(|conn| {
                conn.revert_last_migration(MIGRATIONS)
                    .map(|v| v.to_string())
                    .map_err(diesel::result::Error::QueryBuilderError)
            });

            match result {
                Ok(version) => {
                    reverted.push(version);
                }
                Err(e) => {
                    tracing::warn!("Migration rollback stopped: {e:?}");
                    break;
                }
            }
        }

        Ok(reverted)
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
    const RECEIVED_PROPOSALS: &str = "202609280000000000";
    const SENDER_SUMMARY: &str = "20260928000001";

    /// Roll back to `target` on a new database and return what remains applied.
    async fn remaining_after_rollback(target: &str) -> Result<Vec<String>, ConnectionError> {
        let store = crate::TestDb::create_ephemeral_store().await;
        let conn = store.db();
        conn.rollback_to_version(target)?;
        conn.applied_migrations()
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn rollback_stops_at_a_shorter_version_in_diesel_order() {
        let remaining = remaining_after_rollback(SENDER_SUMMARY).await?;
        assert_eq!(remaining, [RECEIVED_PROPOSALS, BASELINE]);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn rollback_reaches_a_longer_version_in_diesel_order() {
        let remaining = remaining_after_rollback(RECEIVED_PROPOSALS).await?;
        assert_eq!(remaining, [BASELINE]);
    }
}
