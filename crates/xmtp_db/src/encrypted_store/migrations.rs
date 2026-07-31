#[cfg(feature = "sync")]
use diesel::migration::{Migration, MigrationSource, MigrationVersion};
#[cfg(feature = "sync")]
use diesel_migrations::MigrationHarness;

#[cfg(feature = "sync")]
use super::{ConnectionExt, MIGRATIONS, Sqlite, db_connection::DbConnection};
use crate::ConnectionError;

/// Trait for database migration operations.
///
/// WARNING: These operations are dangerous and can cause data loss.
/// They are intended for debugging and admin tools only.
#[maybe_async::maybe_async(AFIT)]
pub trait QueryMigrations {
    /// Returns a list of all applied migration versions, most recent first.
    async fn applied_migrations(&self) -> Result<Vec<String>, ConnectionError>;

    /// Returns a list of all available (embedded) migration names.
    async fn available_migrations(&self) -> Result<Vec<String>, ConnectionError>;

    /// Rollback all migrations after and including the specified version.
    ///
    /// WARNING: This is destructive and may cause data loss.
    async fn rollback_to_version(&self, version: &str) -> Result<Vec<String>, ConnectionError>;

    /// Run a specific migration by name.
    ///
    /// NOTE: This runs the migration SQL directly without updating the
    /// schema_migrations tracking table.
    async fn run_migration(&self, name: &str) -> Result<(), ConnectionError>;

    /// Revert a specific migration by name.
    ///
    /// NOTE: This runs the revert SQL directly without updating the
    /// schema_migrations tracking table.
    async fn revert_migration(&self, name: &str) -> Result<(), ConnectionError>;

    /// Run all pending migrations.
    async fn run_pending_migrations(&self) -> Result<Vec<String>, ConnectionError>;
}

#[cfg(feature = "sync")]
fn get_migrations() -> Result<Vec<Box<dyn Migration<Sqlite>>>, ConnectionError> {
    MigrationSource::<Sqlite>::migrations(&MIGRATIONS)
        .map_err(|e| ConnectionError::Database(diesel::result::Error::QueryBuilderError(e)))
}

#[cfg(feature = "sync")]
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
        let target: String = version.chars().filter(|c| c.is_numeric()).collect();
        let target: u64 = target.parse().map_err(|_| {
            ConnectionError::InvalidQuery(format!("Invalid migration version: {version}"))
        })?;

        let mut reverted = Vec::new();

        loop {
            let applied = self.applied_migrations()?;
            let Some(current_version) = applied.first() else {
                break;
            };

            let version_number: String =
                current_version.chars().filter(|c| c.is_numeric()).collect();
            let current_num: u64 = version_number.parse().map_err(|_| {
                ConnectionError::InvalidQuery(format!("Invalid applied version: {current_version}"))
            })?;

            if current_num < target {
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

/// The Postgres migration set, embedded at compile time.
///
/// The sync track gets this from `diesel_migrations::embed_migrations!`, which
/// is SQLite-only. There is deliberately no runtime directory scan: a server
/// must carry its schema in its binary, so it cannot be pointed at a stale or
/// hand-edited copy.
#[cfg(all(feature = "async", not(target_arch = "wasm32")))]
pub mod pg {
    /// One embedded migration.
    ///
    /// `version` is the numeric prefix and `name` the full directory name,
    /// matching diesel's split -- [`super::QueryMigrations::applied_migrations`]
    /// reports versions while `available_migrations` reports names, and
    /// `rollback_to_version` compares versions numerically.
    pub struct PgMigration {
        pub name: &'static str,
        pub version: &'static str,
        pub up: &'static str,
        pub down: &'static str,
    }

    /// Every Postgres migration, oldest first. Postgres has no deployed history
    /// to inherit, so unlike SQLite's 64 migrations this starts from one
    /// consolidated schema.
    pub const MIGRATIONS: &[PgMigration] = &[PgMigration {
        name: "0000_init",
        version: "0000",
        up: include_str!("../../migrations_pg/0000_init/up.sql"),
        down: include_str!("../../migrations_pg/0000_init/down.sql"),
    }];

    /// Where applied versions are recorded.
    ///
    /// Deliberately named apart from diesel's `__diesel_schema_migrations` and
    /// sqlx's `_sqlx_migrations`: nothing should collide if either tool is ever
    /// pointed at the same database.
    pub const TRACKING_TABLE: &str = "__xmtp_schema_migrations";

    /// DDL for [`TRACKING_TABLE`], run before every read of it.
    ///
    /// `run_at_ns` is nanoseconds since the epoch rather than a `TIMESTAMPTZ`,
    /// matching how every other timestamp in this schema is stored.
    pub fn tracking_table_ddl() -> String {
        format!(
            "CREATE TABLE IF NOT EXISTS {TRACKING_TABLE} (\
                 version TEXT PRIMARY KEY NOT NULL, \
                 run_at_ns BIGINT NOT NULL)"
        )
    }

    /// SQL that marks every embedded migration as already applied.
    ///
    /// For callers that install the schema by running `up.sql` directly (the
    /// test harness does) rather than through the runner. Without it the two
    /// paths would disagree about what is applied, and a later
    /// `run_pending_migrations` would try to re-create tables that exist.
    pub fn baseline_sql(run_at_ns: i64) -> String {
        let rows = MIGRATIONS
            .iter()
            .map(|m| format!("('{}', {run_at_ns})", m.version))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{}; INSERT INTO {TRACKING_TABLE} (version, run_at_ns) VALUES {rows} \
             ON CONFLICT (version) DO NOTHING",
            tracking_table_ddl()
        )
    }
}

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sync")`.
///
/// This is the one trait whose async form is not a port of the diesel impl:
/// every sync method delegates to diesel's embedded-migration machinery
/// (`MigrationHarness`, `Migration<Sqlite>`), which has no Postgres analogue
/// here. What it delegates *to* -- a tracking table plus apply/revert of
/// embedded SQL -- is small enough to implement directly, and is what makes the
/// async track's schema deployable at all rather than only creatable by a test
/// fixture.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
mod pg_impl {
    use super::*;
    use crate::pg::PgDb;
    use pg::{MIGRATIONS, PgMigration, TRACKING_TABLE, tracking_table_ddl};
    use xmtp_common::time::now_ns;

    /// The numeric part of a version or name, as the sync path parses it:
    /// `"0000_init"` and `"0000"` both mean 0.
    fn version_number(value: &str) -> Result<u64, ConnectionError> {
        let digits: String = value.chars().filter(|c| c.is_numeric()).collect();
        digits.parse().map_err(|_| {
            ConnectionError::InvalidQuery(format!("Invalid migration version: {value}"))
        })
    }

    fn find(name: &str) -> Result<&'static PgMigration, ConnectionError> {
        MIGRATIONS
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| ConnectionError::InvalidQuery(format!("Migration not found: {name}")))
    }

    impl PgDb {
        /// Runs `sql` and records `version`, both or neither.
        ///
        /// Postgres DDL is transactional, so a migration that fails halfway
        /// leaves nothing behind -- the property SQLite cannot offer and the
        /// reason this does not need the "did it get partway?" recovery the
        /// sync path's harness has.
        async fn apply_migration(
            &self,
            migration: &PgMigration,
            direction_sql: &str,
            record: bool,
        ) -> Result<(), ConnectionError> {
            self.atomic(async |db| {
                {
                    let mut c = db.conn().await?;
                    // `raw_sql` uses the simple query protocol, which is what
                    // allows a whole multi-statement file (including `$$`-quoted
                    // function bodies) in one call.
                    sqlx::raw_sql(direction_sql).execute(&mut *c).await?;
                }
                if record {
                    let mut c = db.conn().await?;
                    sqlx::query(&format!(
                        "INSERT INTO {TRACKING_TABLE} (version, run_at_ns) VALUES ($1, $2) \
                         ON CONFLICT (version) DO NOTHING"
                    ))
                    .bind(migration.version)
                    .bind(now_ns())
                    .execute(&mut *c)
                    .await?;
                }
                Ok(())
            })
            .await
        }

        async fn forget_migration(&self, version: &str) -> Result<(), ConnectionError> {
            let mut c = self.conn().await?;
            sqlx::query(&format!("DELETE FROM {TRACKING_TABLE} WHERE version = $1"))
                .bind(version)
                .execute(&mut *c)
                .await?;
            Ok(())
        }
    }

    impl QueryMigrations for PgDb {
        /// Creates the tracking table if it is missing, so this is safe to call
        /// against a database the runner has never touched.
        async fn applied_migrations(&self) -> Result<Vec<String>, ConnectionError> {
            let mut c = self.conn().await?;
            sqlx::raw_sql(&tracking_table_ddl())
                .execute(&mut *c)
                .await?;
            Ok(sqlx::query_scalar(&format!(
                "SELECT version FROM {TRACKING_TABLE} ORDER BY version DESC"
            ))
            .fetch_all(&mut *c)
            .await?)
        }

        async fn available_migrations(&self) -> Result<Vec<String>, ConnectionError> {
            Ok(MIGRATIONS.iter().map(|m| m.name.to_string()).collect())
        }

        /// Reverts every applied migration at or after `version`, newest first,
        /// stopping at the first failure rather than unwinding further -- the
        /// same shape as the sync path's loop.
        async fn rollback_to_version(&self, version: &str) -> Result<Vec<String>, ConnectionError> {
            let target = version_number(version)?;
            let mut reverted = Vec::new();

            loop {
                let applied = self.applied_migrations().await?;
                let Some(current) = applied.first() else {
                    break;
                };
                if version_number(current)? < target {
                    break;
                }

                let Some(migration) = MIGRATIONS.iter().find(|m| m.version == current) else {
                    // Applied by a build that knows a migration this one does
                    // not; reverting it is not something we can do.
                    tracing::warn!("Migration rollback stopped: no embedded migration {current}");
                    break;
                };

                match self.apply_migration(migration, migration.down, false).await {
                    Ok(()) => {
                        self.forget_migration(migration.version).await?;
                        reverted.push(migration.version.to_string());
                    }
                    Err(e) => {
                        tracing::warn!("Migration rollback stopped: {e:?}");
                        break;
                    }
                }
            }

            Ok(reverted)
        }

        async fn run_migration(&self, name: &str) -> Result<(), ConnectionError> {
            let migration = find(name)?;
            self.apply_migration(migration, migration.up, false).await
        }

        async fn revert_migration(&self, name: &str) -> Result<(), ConnectionError> {
            let migration = find(name)?;
            self.apply_migration(migration, migration.down, false).await
        }

        async fn run_pending_migrations(&self) -> Result<Vec<String>, ConnectionError> {
            let applied = self.applied_migrations().await?;
            let mut ran = Vec::new();

            for migration in MIGRATIONS {
                if applied.iter().any(|v| v == migration.version) {
                    continue;
                }
                self.apply_migration(migration, migration.up, true).await?;
                ran.push(migration.version.to_string());
            }

            Ok(ran)
        }
    }
}
