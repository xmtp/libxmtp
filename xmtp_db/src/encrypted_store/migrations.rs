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

/// Extract the numeric version from a migration name for comparison.
/// Migration names follow the format: YYYY-MM-DD-HHMMSS[optional suffix]_name
/// We extract only the numeric characters from the version portion (before any underscore
/// followed by non-numeric characters) for robust comparison regardless of formatting.
fn extract_numeric_version(version: &str) -> String {
    // Find where the descriptive name starts (after the version prefix)
    // The version portion ends at the first underscore that's followed by a letter
    let version_end = version
        .char_indices()
        .zip(version.chars().skip(1))
        .find(|((_, c), next)| *c == '_' && next.is_alphabetic())
        .map(|((i, _), _)| i)
        .unwrap_or(version.len());

    // Extract only numeric characters from the version portion
    version[..version_end]
        .chars()
        .filter(|c| c.is_numeric())
        .collect()
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
        // Extract numeric version for comparison
        // This handles different migration naming formats consistently
        let target_numeric = extract_numeric_version(version);

        tracing::debug!("Rolling back to version: {version} (numeric: {target_numeric})");

        let mut reverted = Vec::new();

        loop {
            // Get applied migrations and find the newest one.
            let applied = self.applied_migrations()?;
            if applied.is_empty() {
                tracing::debug!("No more applied migrations, stopping rollback");
                break;
            }

            // Find the newest migration by comparing numeric versions.
            // This is robust regardless of formatting differences (dashes, etc.)
            let Some(newest) = applied
                .iter()
                .max_by(|a, b| extract_numeric_version(a).cmp(&extract_numeric_version(b)))
            else {
                // This should never happen since we checked applied.is_empty() above,
                // but handle it gracefully just in case.
                tracing::debug!("No applied migrations found after empty check, stopping rollback");
                break;
            };
            let newest_numeric = extract_numeric_version(newest);

            tracing::debug!("Newest applied migration: {newest} (numeric: {newest_numeric})");

            // Use numeric comparison on the version
            // Migration versions are numeric timestamps that sort correctly
            // Use <= to ensure the target migration itself is kept applied
            if newest_numeric <= target_numeric {
                tracing::debug!("Stopping rollback: {newest_numeric} <= {target_numeric}");
                break;
            }

            tracing::debug!("Reverting migration: {newest}");

            // Revert the newest migration via Diesel's revert_last_migration.
            // Note: Diesel determines "last" based on its own ordering, which
            // should match our numeric ordering of versions.
            let result = self.raw_query_write(|conn| {
                conn.revert_last_migration(MIGRATIONS)
                    .map(|v| v.to_string())
                    .map_err(diesel::result::Error::QueryBuilderError)
            });

            match result {
                Ok(reverted_version) => {
                    let reverted_numeric = extract_numeric_version(&reverted_version);

                    // Verify we reverted what we expected to revert
                    if reverted_numeric != newest_numeric {
                        tracing::warn!(
                            "Migration ordering mismatch: expected to revert {} (numeric: {}), but reverted {} (numeric: {}). \
                            This may indicate that Diesel's migration ordering differs from our numeric ordering.",
                            newest,
                            newest_numeric,
                            reverted_version,
                            reverted_numeric
                        );

                        // If what was reverted is at or before our target, we should stop
                        // to avoid reverting too much
                        if reverted_numeric <= target_numeric {
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
