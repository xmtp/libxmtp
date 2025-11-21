use anyhow::Result;
use diesel_migrations::MigrationHarness;
use tracing::info;
use xmtp_db::{EncryptedMessageStore, NativeDb, diesel::migration::MigrationVersion};

use crate::confirm_destructive;

pub fn revert_migrations(store: &EncryptedMessageStore<NativeDb>, target: &str) -> Result<()> {
    confirm_destructive()?;

    let target: String = target.chars().filter(|c| c.is_numeric()).collect();
    while let Some(version) = applied_migrations(store)?.first() {
        if version.to_string() > target {
            let result = store.db().raw_query_write(|conn| {
                Ok(conn.revert_last_migration(xmtp_db::MIGRATIONS).unwrap())
            });
            if let Err(err) = result {
                tracing::warn!("{err:?}");
            } else {
                info!("Reverted {version}");
            }
        } else {
            break;
        }
    }

    Ok(())
}

fn applied_migrations(
    store: &EncryptedMessageStore<NativeDb>,
) -> Result<Vec<MigrationVersion<'static>>> {
    let applied_migrations: Vec<MigrationVersion<'static>> = store
        .db()
        .raw_query_read(|conn| Ok(conn.applied_migrations().unwrap()))?;
    Ok(applied_migrations)
}
