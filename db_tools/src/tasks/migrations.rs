use crate::confirm_destructive;
use anyhow::Result;
use tracing::info;
use xmtp_db::{ConnectionExt, DbConnection, migrations::QueryMigrations};

pub fn rollback(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    confirm_destructive()?;
    rollback_confirmed(conn, target)
}

pub fn rollback_confirmed(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    let db = DbConnection::new(conn);
    let reverted = db.rollback_to_version(target)?;
    for version in &reverted {
        info!("Reverted {version}");
    }
    Ok(())
}

pub fn run_migration(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    confirm_destructive()?;
    run_migration_confirmed(conn, target)
}

pub fn run_migration_confirmed(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    let db = DbConnection::new(conn);
    info!("Running migration for {target}...");
    db.run_migration(target)?;
    Ok(())
}

pub fn revert_migration(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    confirm_destructive()?;
    revert_migration_confirmed(conn, target)
}

pub fn revert_migration_confirmed(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    let db = DbConnection::new(conn);
    info!("Reverting migration {target}...");
    db.revert_migration(target)?;
    Ok(())
}

#[allow(dead_code)] // Used in tests
pub fn applied_migrations(conn: &impl ConnectionExt) -> Result<Vec<String>> {
    let db = DbConnection::new(conn);
    Ok(db.applied_migrations()?)
}

#[cfg(test)]
mod tests {
    use xmtp_db::migrations::QueryMigrations;
    use xmtp_mls::tester;

    use super::{applied_migrations, revert_migration_confirmed, run_migration_confirmed};
    use crate::tasks::rollback_confirmed;

    #[xmtp_common::test(unwrap_try = true)]
    #[ignore]
    async fn test_rollback_and_run_pending_migrations() {
        tester!(alix);
        tester!(bo);

        let (dm, _) = alix.test_talk_in_dm_with(&bo).await?;

        let bo_dm = bo.group(&dm.group_id)?;
        let bo_msgs = bo_dm.find_messages(&Default::default())?;
        assert_eq!(bo_msgs.len(), 2);

        // Go back and forth a couple times
        for _ in 0..2 {
            rollback_confirmed(&alix.db(), "2025-07-08-010431_modify_commit_log")?;
            xmtp_db::DbConnection::new(&alix.db()).run_pending_migrations()?;
        }

        // Verify messaging still works after migrations
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
                applied[i] >= applied[i + 1],
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
        xmtp_db::DbConnection::new(&alix.db()).run_pending_migrations()?;
    }
}
