use crate::confirm_destructive;
use anyhow::Result;
use tracing::info;
use xmtp_db::{ConnectionExt, DbConnection, migrations::QueryMigrations};

/// Reverts `target` and every later migration, and returns the reverted
/// versions, latest first.
pub fn rollback(conn: &impl ConnectionExt, target: &str) -> Result<Vec<String>> {
    confirm_destructive()?;
    rollback_confirmed(conn, target)
}

pub fn rollback_confirmed(conn: &impl ConnectionExt, target: &str) -> Result<Vec<String>> {
    let db = DbConnection::new(conn);
    let reverted = db.rollback_to_version(target)?;
    for version in &reverted {
        info!("Reverted {version}");
    }
    Ok(reverted)
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
    use super::*;
    use xmtp_db::{
        MIGRATIONS, NativeDb, XmtpDb,
        diesel::{
            connection::SimpleConnection,
            migration::{Migration, MigrationSource},
            sqlite::Sqlite,
        },
    };

    // Migrations that have shipped. Migrations are append-only, so these stay
    // the first embedded names, in this order. A new migration sorts after them.
    const SHIPPED: [&str; 8] = [
        "2026-09-08-000000_baseline",
        "2026-09-24-000000_attachments",
        "2026-09-28-000000-0000_received_proposals",
        "2026-09-28-010000_conversation_list_expiry",
        "2026-09-29-000000_conversation_list_latest_message_index",
        "2026-10-09-000000_history_position",
        "2026-10-09-010000_pending_history_position",
        "2026-10-09-020000_group_reference",
    ];

    // Kept migrations whose schema changes the test can observe.
    const ATTACHMENTS: &str = "20260924000000";
    const CONVERSATION_LIST_EXPIRY: &str = "20260928010000";

    /// The embedded migration names, sorted, and their versions in the order
    /// Diesel reports applied versions: string order, latest first.
    fn embedded_migrations() -> (Vec<String>, Vec<String>) {
        let migrations: Vec<Box<dyn Migration<Sqlite>>> =
            MigrationSource::<Sqlite>::migrations(&MIGRATIONS).expect("embedded migrations load");
        let mut names: Vec<String> = migrations.iter().map(|m| m.name().to_string()).collect();
        names.sort();
        let mut versions: Vec<String> = migrations
            .iter()
            .map(|m| m.name().version().to_string())
            .collect();
        versions.sort_by(|a, b| b.cmp(a));
        (names, versions)
    }

    /// Splits the applied versions at a rollback to `target`: the versions it
    /// reverts (the target and every later one, latest first) and the versions
    /// left applied (every version that Diesel orders before it).
    fn rollback_split(applied: &[String], target: &str) -> (Vec<String>, Vec<String>) {
        let index = applied
            .iter()
            .position(|version| version == target)
            .unwrap_or_else(|| panic!("{target} is an embedded migration"));
        let (reverted, remaining) = applied.split_at(index + 1);
        (reverted.to_vec(), remaining.to_vec())
    }

    // Schema facts this test checks: the latest schema has no
    // `conversation_list`, and has `local_attachments(mime_type, filename)` and
    // `pending_attachments`. A rollback to CONVERSATION_LIST_EXPIRY restores
    // `conversation_list`, and a rollback to ATTACHMENTS drops both attachment
    // tables. A new migration that changes any of these must update this test.
    #[xmtp_common::test(unwrap_try = true)]
    async fn latest_migration_status_and_rollback() {
        let database = NativeDb::builder().ephemeral().build_unencrypted()?;
        database.init()?;
        let conn = database.conn();
        let db = DbConnection::new(&conn);
        let (expected_available, expected_applied) = embedded_migrations();

        let mut available = db.available_migrations()?;
        available.sort();
        assert_eq!(available, expected_available);
        assert_eq!(
            available[..SHIPPED.len().min(available.len())],
            SHIPPED,
            "shipped migrations are never removed, renamed, or reordered"
        );
        let applied = applied_migrations(&conn)?;
        assert_eq!(applied, expected_applied);
        assert!(
            conn.raw_query(|c| c.batch_execute("SELECT * FROM conversation_list"))
                .is_err()
        );

        // Each rollback names a target version and reverts it and every later one.
        let (reverted, remaining) = rollback_split(&applied, CONVERSATION_LIST_EXPIRY);
        assert_eq!(
            rollback_confirmed(&conn, CONVERSATION_LIST_EXPIRY)?,
            reverted
        );
        assert_eq!(applied_migrations(&conn)?, remaining);
        conn.raw_query(|c| c.batch_execute("SELECT * FROM conversation_list"))?;

        let (reverted, remaining) = rollback_split(&remaining, ATTACHMENTS);
        assert_eq!(rollback_confirmed(&conn, ATTACHMENTS)?, reverted);
        assert_eq!(applied_migrations(&conn)?, remaining);
        assert!(
            conn.raw_query(|c| c.batch_execute("SELECT * FROM local_attachments"))
                .is_err()
        );
        assert!(
            conn.raw_query(|c| c.batch_execute("SELECT * FROM pending_attachments"))
                .is_err()
        );

        let baseline = applied.last().expect("the baseline is applied");
        assert_eq!(
            rollback_confirmed(&conn, baseline)?,
            std::slice::from_ref(baseline)
        );
        assert!(applied_migrations(&conn)?.is_empty());

        // Pending migrations run oldest first.
        let oldest_first: Vec<String> = applied.iter().rev().cloned().collect();
        assert_eq!(db.run_pending_migrations()?, oldest_first);
        assert_eq!(applied_migrations(&conn)?, applied);
        assert!(
            conn.raw_query(|c| c.batch_execute("SELECT * FROM conversation_list"))
                .is_err()
        );
        conn.raw_query(|c| c.batch_execute("SELECT mime_type, filename FROM local_attachments"))?;
        conn.raw_query(|c| c.batch_execute("SELECT * FROM pending_attachments"))?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn direct_baseline_run_and_revert_preserve_tracking() {
        let database = NativeDb::builder().ephemeral().build_unencrypted()?;
        database.init()?;
        let conn = database.conn();
        let db = DbConnection::new(&conn);
        let baseline = db
            .available_migrations()?
            .into_iter()
            .find(|name| name.ends_with("_baseline"))
            .expect("baseline migration exists");
        let applied = applied_migrations(&conn)?;
        revert_migration_confirmed(&conn, &baseline)?;
        assert_eq!(applied_migrations(&conn)?, applied);
        assert!(
            conn.raw_query(|c| c.batch_execute("SELECT * FROM conversation_list"))
                .is_err()
        );
        run_migration_confirmed(&conn, &baseline)?;
        conn.raw_query(|c| c.batch_execute("SELECT * FROM conversation_list"))?;
        assert_eq!(applied_migrations(&conn)?, applied);
    }
}
