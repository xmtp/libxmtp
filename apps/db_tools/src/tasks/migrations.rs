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
    use xmtp_db::{NativeDb, XmtpDb, diesel::connection::SimpleConnection};

    #[xmtp_common::test(unwrap_try = true)]
    async fn latest_migration_status_and_rollback() {
        let database = NativeDb::builder().ephemeral().build_unencrypted()?;
        database.init()?;
        let conn = database.conn();
        let db = DbConnection::new(&conn);
        let mut available = db.available_migrations()?;
        available.sort();
        assert_eq!(
            available,
            [
                "2026-09-08-000000_baseline",
                "2026-09-24-000000_attachments",
                "2026-09-28-000000-0000_received_proposals",
                "2026-09-28-010000_conversation_list_expiry",
                "2026-09-28-030000_restored_group_metadata",
            ]
        );
        let applied = applied_migrations(&conn)?;
        assert_eq!(
            applied,
            [
                "20260928030000",
                "20260928010000",
                "202609280000000000",
                "20260924000000",
                "20260908000000"
            ]
        );
        assert!(
            conn.raw_query(|c| c.batch_execute("SELECT * FROM conversation_list"))
                .is_err()
        );

        // Each rollback names a target version and reverts it and every later one.
        rollback_confirmed(&conn, "20260928010000")?;
        assert_eq!(applied_migrations(&conn)?, applied[2..].to_vec());
        conn.raw_query(|c| c.batch_execute("SELECT * FROM conversation_list"))?;

        rollback_confirmed(&conn, "20260924000000")?;
        assert_eq!(applied_migrations(&conn)?, applied[4..].to_vec());
        assert!(
            conn.raw_query(|c| c.batch_execute("SELECT * FROM local_attachments"))
                .is_err()
        );
        assert!(
            conn.raw_query(|c| c.batch_execute("SELECT * FROM pending_attachments"))
                .is_err()
        );

        rollback_confirmed(&conn, &applied[4])?;
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
