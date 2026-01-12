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

    /// Determine if a migration is applied based on its name and the list of applied versions.
    fn migration_status(name: &str, applied: &[String]) -> &'static str {
        let name_version: String = name.chars().filter(|c| c.is_numeric()).collect();
        if applied.iter().any(|a| {
            let applied_version: String = a.chars().filter(|c| c.is_numeric()).collect();
            name_version == applied_version
        }) {
            "[applied]"
        } else {
            "[pending]"
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    #[ignore]
    async fn test_rollback_and_run_pending_migrations() {
        tester!(alix, persistent_db);
        tester!(bo);

        let (dm, _) = alix.test_talk_in_dm_with(&bo).await?;

        let bo_dm = bo.group(&dm.group_id)?;
        let bo_msgs = bo_dm.find_messages(&Default::default())?;
        assert_eq!(bo_msgs.len(), 2);

        // Get a migration from the middle of the list to rollback to
        let conn = alix.db();
        let db = xmtp_db::DbConnection::new(&conn);
        let mut available = db.available_migrations()?;
        available.sort_by(|a, b| b.cmp(a));
        let rollback_target = &available[available.len() / 2];

        // Go back and forth a couple times
        for _ in 0..2 {
            rollback_confirmed(&alix.db(), rollback_target)?;
            db.run_pending_migrations()?;
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

        // Use a simple migration that just adds a column (no table recreation)
        // This avoids complex schema dependencies that can cause issues with rollback ordering
        let target_migration = "2026-01-09-000000_add_should_push_to_group_messages";
        // The migration immediately before our target - rollback keeps this one applied
        let rollback_to = "2025-12-19-153956-0000_add_dm_group_updates_migrated";

        // First rollback to before the target migration (rollback_to is kept, target is reverted)
        rollback_confirmed(&alix.db(), rollback_to)?;

        // Helper to check if column exists using raw SQL
        fn check_column_exists(conn: &impl xmtp_db::ConnectionExt) -> anyhow::Result<bool> {
            Ok(conn.raw_query_read(|c| {
                use xmtp_db::diesel::connection::SimpleConnection;
                // Try to select the column - if it fails, the column doesn't exist
                let result = c.batch_execute("SELECT should_push FROM group_messages LIMIT 0");
                Ok(result.is_ok())
            })?)
        }

        // Verify the should_push column doesn't exist after rollback
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

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_migration_status_applied_and_pending() {
        tester!(alix, persistent_db);

        let conn = alix.db();
        let db = xmtp_db::DbConnection::new(&conn);

        // Use specific known migrations to avoid fragile array index dependencies
        // and potential ordering mismatches between our code and Diesel
        let newest = "2026-01-09-000000_add_should_push_to_group_messages";
        let rollback_target = "2025-12-19-153956-0000_add_dm_group_updates_migrated";
        let before_target = "2025-12-08-160215-0000_drop_events_table";
        let oldest = "2024-05-06-192337_openmls_storage";

        let applied_before = db.applied_migrations()?;

        // All should be applied initially
        for name in [newest, rollback_target, before_target, oldest] {
            assert_eq!(
                migration_status(name, &applied_before),
                "[applied]",
                "{name} should be applied initially"
            );
        }

        // Rollback to rollback_target - it stays applied, newest becomes pending
        rollback_confirmed(&alix.db(), rollback_target)?;

        let applied_after = db.applied_migrations()?;

        // Rollback target and older migrations should still be applied
        assert_eq!(
            migration_status(rollback_target, &applied_after),
            "[applied]",
            "{rollback_target} should still be applied (it's the rollback target)"
        );
        assert_eq!(
            migration_status(before_target, &applied_after),
            "[applied]",
            "{before_target} should still be applied after rollback"
        );
        assert_eq!(
            migration_status(oldest, &applied_after),
            "[applied]",
            "{oldest} should still be applied after rollback"
        );

        // Migrations after rollback target should be pending
        assert_eq!(
            migration_status(newest, &applied_after),
            "[pending]",
            "{newest} should be pending after rollback"
        );

        // Restore full state
        db.run_pending_migrations()?;

        let applied_restored = db.applied_migrations()?;

        // All migrations should be applied again
        assert_eq!(
            migration_status(rollback_target, &applied_restored),
            "[applied]",
            "{rollback_target} should be applied after restore"
        );
        assert_eq!(
            migration_status(newest, &applied_restored),
            "[applied]",
            "{newest} should be applied after restore"
        );
    }
}
