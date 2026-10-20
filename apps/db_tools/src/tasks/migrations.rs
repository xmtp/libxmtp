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
    use super::*;
    use xmtp_db::{
        MIGRATIONS, NativeDb, XmtpDb,
        diesel::{
            connection::SimpleConnection,
            migration::{Migration, MigrationSource},
            sqlite::Sqlite,
        },
    };

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

    /// The applied versions left after a rollback to `target`: every version
    /// that Diesel orders before it.
    fn remaining_after(applied: &[String], target: &str) -> Vec<String> {
        let index = applied
            .iter()
            .position(|version| version == target)
            .unwrap_or_else(|| panic!("{target} is an embedded migration"));
        applied[index + 1..].to_vec()
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn latest_migration_status_and_rollback() {
        let database = NativeDb::builder().ephemeral().build_unencrypted()?;
        database.init()?;
        let conn = database.conn();
        let db = DbConnection::new(&conn);
        let (expected_available, expected_applied) = embedded_migrations();
        assert!(
            expected_available
                .iter()
                .any(|name| name.ends_with("_baseline"))
        );

        let mut available = db.available_migrations()?;
        available.sort();
        assert_eq!(available, expected_available);
        let applied = applied_migrations(&conn)?;
        assert_eq!(applied, expected_applied);
        assert!(
            conn.raw_query(|c| c.batch_execute("SELECT * FROM conversation_list"))
                .is_err()
        );

        // Each rollback names a target version and reverts it and every later one.
        rollback_confirmed(&conn, CONVERSATION_LIST_EXPIRY)?;
        assert_eq!(
            applied_migrations(&conn)?,
            remaining_after(&applied, CONVERSATION_LIST_EXPIRY)
        );
        conn.raw_query(|c| c.batch_execute("SELECT * FROM conversation_list"))?;

        rollback_confirmed(&conn, ATTACHMENTS)?;
        assert_eq!(
            applied_migrations(&conn)?,
            remaining_after(&applied, ATTACHMENTS)
        );
        assert!(
            conn.raw_query(|c| c.batch_execute("SELECT * FROM local_attachments"))
                .is_err()
        );
        assert!(
            conn.raw_query(|c| c.batch_execute("SELECT * FROM pending_attachments"))
                .is_err()
        );

        let baseline = applied.last().expect("the baseline is applied");
        rollback_confirmed(&conn, baseline)?;
        assert!(applied_migrations(&conn)?.is_empty());

        db.run_pending_migrations()?;
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
