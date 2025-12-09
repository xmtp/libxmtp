use crate::confirm_destructive;
use anyhow::Result;
use diesel_migrations::MigrationHarness;
use tracing::info;
use xmtp_db::{
    ConnectionExt, Sqlite,
    diesel::{
        migration::{Migration, MigrationSource, MigrationVersion},
        result::Error as DieselError,
    },
};

pub fn rollback(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    confirm_destructive()?;
    rollback_confirmed(conn, target)
}

pub fn rollback_confirmed(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    let target: String = target.chars().filter(|c| c.is_numeric()).collect();
    let target: u64 = target.parse()?;
    while let Some(version) = applied_migrations(conn)?.first() {
        let version = version.to_string();
        let version_number: String = version.chars().filter(|c| c.is_numeric()).collect();
        if version_number.parse::<u64>()? >= target {
            let result = conn.raw_query_write(|conn| {
                Ok(conn
                    .revert_last_migration(xmtp_db::MIGRATIONS)
                    .map_err(DieselError::QueryBuilderError))
            });
            if let Err(err) = result {
                tracing::warn!("{err:?}");
                break;
            } else {
                info!("Reverted {version}");
            }
        } else {
            break;
        }
    }

    Ok(())
}

pub fn run_migration(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    confirm_destructive()?;
    run_migration_confirmed(conn, target)
}

fn run_migration_confirmed(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    let migrations: Vec<Box<dyn Migration<Sqlite>>> = xmtp_db::MIGRATIONS.migrations().unwrap();

    for migration in migrations {
        if migration.name().to_string() != target {
            continue;
        }

        info!("Running migration for {target}...");
        conn.raw_query_write(|c| migration.run(c).map_err(DieselError::QueryBuilderError))?;
    }

    Ok(())
}

pub fn revert_migration(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    confirm_destructive()?;
    revert_migration_confirmed(conn, target)
}

pub fn revert_migration_confirmed(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    let migrations: Vec<Box<dyn Migration<Sqlite>>> = xmtp_db::MIGRATIONS.migrations().unwrap();

    for migration in migrations {
        if migration.name().to_string() != target {
            continue;
        }

        info!("Running migration for {target}...");
        conn.raw_query_write(|c| migration.revert(c).map_err(DieselError::QueryBuilderError))?;
    }

    Ok(())
}

fn applied_migrations(conn: &impl ConnectionExt) -> Result<Vec<MigrationVersion<'static>>> {
    let applied_migrations: Vec<MigrationVersion<'static>> = conn.raw_query_read(|conn| {
        conn.applied_migrations()
            .map_err(DieselError::QueryBuilderError)
    })?;
    Ok(applied_migrations)
}

#[cfg(test)]
mod tests {
    use diesel_migrations::MigrationHarness;
    use xmtp_mls::tester;

    use super::{applied_migrations, revert_migration_confirmed, run_migration_confirmed};
    use crate::tasks::rollback_confirmed;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_rollback_and_run_pending_migrations() {
        tester!(alix, persistent_db);
        tester!(bo);

        let (dm, _) = alix.test_talk_in_dm_with(&bo).await?;

        let bo_dm = bo.group(&dm.group_id)?;
        let bo_msgs = bo_dm.find_messages(&Default::default())?;
        assert_eq!(bo_msgs.len(), 2);

        // Go back and forth a couple times
        for _ in 0..2 {
            rollback_confirmed(&alix.db(), "2025-07-08-010431_modify_commit_log")?;
            alix.db().raw_query_write(|c| {
                c.run_pending_migrations(xmtp_db::MIGRATIONS)?;
                Ok(())
            })?;
        }

        alix.test_talk_in_dm_with(&bo).await?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_applied_migrations_returns_versions() {
        tester!(alix);

        let applied = applied_migrations(&alix.db())?;

        // Should have migrations applied (the tester applies all migrations)
        assert!(!applied.is_empty());

        // Versions should be in descending order (most recent first)
        for i in 0..applied.len().saturating_sub(1) {
            assert!(
                applied[i].to_string() >= applied[i + 1].to_string(),
                "Migrations should be ordered descending"
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_run_and_revert_specific_migration() {
        tester!(alix, persistent_db);

        // Note: run_migration_confirmed and revert_migration_confirmed run the SQL directly
        // without updating the schema_migrations table. This test verifies this behavior
        // by checking if the actual schema changes occur.

        let target_migration = "2025-11-15-232503_add_inserted_at_ns_to_group_messages";
        // The migration before our target - rollback keeps this one applied
        let rollback_to = "2025-10-07-180046_create_tasks";

        // First rollback to before the target migration (rollback_to is kept, target is reverted)
        rollback_confirmed(&alix.db(), rollback_to)?;

        // Helper to check if column exists using raw SQL
        fn check_column_exists(conn: &impl xmtp_db::ConnectionExt) -> anyhow::Result<bool> {
            Ok(conn.raw_query_read(|c| {
                use xmtp_db::diesel::connection::SimpleConnection;
                // Try to select the column - if it fails, the column doesn't exist
                let result = c.batch_execute("SELECT inserted_at_ns FROM group_messages LIMIT 0");
                Ok(result.is_ok())
            })?)
        }

        // Verify the inserted_at_ns column doesn't exist after rollback
        assert!(
            !check_column_exists(&alix.db())?,
            "Column should not exist after rollback"
        );

        // Run the specific migration (runs SQL directly, doesn't update tracking table)
        run_migration_confirmed(&alix.db(), target_migration)?;

        // Verify the column now exists
        assert!(
            check_column_exists(&alix.db())?,
            "Column should exist after running migration"
        );

        // Revert the specific migration (runs SQL directly)
        revert_migration_confirmed(&alix.db(), target_migration)?;

        // Verify the column is removed
        assert!(
            !check_column_exists(&alix.db())?,
            "Column should not exist after reverting migration"
        );

        // Run pending migrations to restore full state
        alix.db().raw_query_write(|c| {
            c.run_pending_migrations(xmtp_db::MIGRATIONS)?;
            Ok(())
        })?;
    }
}
