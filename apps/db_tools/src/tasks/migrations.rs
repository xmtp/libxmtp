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
    async fn baseline_migration_status_and_rollback() {
        let database = NativeDb::builder().ephemeral().build_unencrypted()?;
        database.init()?;
        let conn = database.conn();
        let db = DbConnection::new(&conn);
        let available = db.available_migrations()?;
        assert_eq!(available.len(), 1);
        let applied = applied_migrations(&conn)?;
        assert_eq!(applied.len(), 1);
        rollback_confirmed(&conn, &applied[0])?;
        assert!(applied_migrations(&conn)?.is_empty());
        assert!(
            conn.raw_query(|c| c.batch_execute("SELECT * FROM conversation_list"))
                .is_err()
        );
        db.run_pending_migrations()?;
        assert_eq!(applied_migrations(&conn)?, applied);
        conn.raw_query(|c| c.batch_execute("SELECT * FROM conversation_list"))?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn direct_baseline_run_and_revert_preserve_tracking() {
        let database = NativeDb::builder().ephemeral().build_unencrypted()?;
        database.init()?;
        let conn = database.conn();
        let db = DbConnection::new(&conn);
        let baseline = db.available_migrations()?.remove(0);
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
