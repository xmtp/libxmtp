use diesel::sql_types::Blob;

use crate::XmtpTestDb;
use diesel::migration::MigrationSource;

use super::*;

#[cfg(not(target_arch = "wasm32"))]
mod add_inserted_at_ns;
mod originator_id_refresh_state;
mod update_dm_trigger;

fn migrate_before(db: impl ConnectionExt, name: &str) {
    migrate(db, name, -1)
}

fn migrate_to(db: impl ConnectionExt, name: &str) {
    migrate(db, name, 0)
}

fn migrate(db: impl ConnectionExt, name: &str, index_change: i32) {
    let migrations = MigrationSource::<Sqlite>::migrations(&MIGRATIONS).unwrap();
    let index = migrations
        .iter()
        .inspect(|m| tracing::info!("{}", m.name()))
        .position(|m| m.name().to_string() == name)
        .unwrap();
    // index is 0-based position, so we need index+1 migrations to include the named one
    // with index_change: -1 = before, 0 = to (including), 1 = after
    let target_index = ((index + 1) as i32 + index_change) as usize;
    db.raw_query_write(|conn| {
        for _ in 0..target_index {
            conn.run_next_migration(MIGRATIONS).unwrap();
        }
        Ok(())
    })
    .unwrap();
}

fn finish_migrations(db: impl ConnectionExt) {
    db.raw_query_write(|conn| {
        conn.run_pending_migrations(MIGRATIONS).unwrap();
        Ok(())
    })
    .unwrap();
}
