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

/// Extract the version prefix from a migration name for comparison.
/// Migration names follow the format: YYYY-MM-DD-HHMMSS[optional suffix]_name
/// We extract the first 17 characters (YYYY-MM-DD-HHMMSS) for comparison.
/// This ensures lexicographic comparison works correctly regardless of suffix format.
fn extract_version_prefix(version: &str) -> &str {
    // Take up to the first 17 characters which covers YYYY-MM-DD-HHMMSS
    // If the version is shorter, take all of it
    let end = version.len().min(17);
    &version[..end]
}

impl<C: ConnectionExt> QueryMigrations for DbConnection<C> {
    fn applied_migrations(&self) -> Result<Vec<String>, ConnectionError> {
        let applied: Vec<MigrationVersion<'static>> = self.raw_query_read(|conn| {
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
        // Extract the date-time prefix for comparison (YYYY-MM-DD-HHMMSS format)
        // We use the first 17 characters which covers the full timestamp portion
        let target_prefix = extract_version_prefix(version);

        tracing::debug!("Rolling back to version: {version} (prefix: {target_prefix})");

        let mut reverted = Vec::new();

        loop {
            // Get applied migrations and find the newest one.
            let applied = self.applied_migrations()?;
            if applied.is_empty() {
                tracing::debug!("No more applied migrations, stopping rollback");
                break;
            }

            // Find the newest migration by comparing all prefixes.
            // This is more robust than sorting because it handles edge cases
            // where migration naming formats might differ.
            let newest = applied
                .iter()
                .max_by(|a, b| {
                    extract_version_prefix(a).cmp(extract_version_prefix(b))
                })
                .unwrap();
            let newest_prefix = extract_version_prefix(newest);

            tracing::debug!(
                "Newest applied migration: {newest} (prefix: {newest_prefix})"
            );

            // Use lexicographic comparison on the version prefix
            // Migration names are formatted as YYYY-MM-DD-HHMMSS so they sort correctly
            // Use <= to ensure the target migration itself is kept applied
            if newest_prefix <= target_prefix {
                tracing::debug!(
                    "Stopping rollback: {newest_prefix} <= {target_prefix}"
                );
                break;
            }

            tracing::debug!("Reverting migration: {newest}");

            // Revert the newest migration via Diesel's revert_last_migration.
            // Note: Diesel determines "last" based on its own ordering, which
            // should match our lexicographic ordering of version prefixes.
            let result = self.raw_query_write(|conn| {
                conn.revert_last_migration(MIGRATIONS)
                    .map(|v| v.to_string())
                    .map_err(diesel::result::Error::QueryBuilderError)
            });

            match result {
                Ok(reverted_version) => {
                    let reverted_prefix = extract_version_prefix(&reverted_version);
                    
                    // Verify we reverted what we expected to revert
                    if reverted_prefix != newest_prefix {
                        tracing::warn!(
                            "Migration ordering mismatch: expected to revert {} (prefix: {}), but reverted {} (prefix: {}). \
                            This may indicate that Diesel's migration ordering differs from our lexicographic ordering.",
                            newest,
                            newest_prefix,
                            reverted_version,
                            reverted_prefix
                        );
                        
                        // If what was reverted is at or before our target, we should stop
                        // to avoid reverting too much
                        if reverted_prefix <= target_prefix {
                            tracing::warn!(
                                "Reverted migration {} is at or before target {}. Stopping to prevent data loss.",
                                reverted_version,
                                version
                            );
                            // Note: The migration was already reverted, so we can't undo it.
                            // We log the situation and stop to prevent further damage.
                            reverted.push(reverted_version);
                            break;
                        }
                    }
                    
                    tracing::debug!("Successfully reverted: {reverted_version}");
                    reverted.push(reverted_version);
                }
                Err(e) => {
                    tracing::warn!("Migration rollback stopped due to error: {e:?}");
                    break;
                }
            }
        }

        tracing::debug!("Rollback complete. Reverted {} migrations", reverted.len());
        Ok(reverted)
    }

    fn run_migration(&self, name: &str) -> Result<(), ConnectionError> {
        let migrations = get_migrations()?;

        for migration in &migrations {
            if migration.name().to_string() == name {
                self.raw_query_write(|c| {
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
                self.raw_query_write(|c| {
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
        let ran: Vec<String> = self.raw_query_write(|conn| {
            conn.run_pending_migrations(MIGRATIONS)
                .map(|versions| versions.into_iter().map(|v| v.to_string()).collect())
                .map_err(diesel::result::Error::QueryBuilderError)
        })?;
        Ok(ran)
    }
}
