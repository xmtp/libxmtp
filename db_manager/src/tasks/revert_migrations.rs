use anyhow::Result;
use diesel_migrations::MigrationHarness;
use tracing::info;
use xmtp_db::{
    ConnectionExt,
    diesel::{migration::MigrationVersion, result::Error as DieselError},
};

use crate::confirm_destructive;

pub fn revert_migrations(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    confirm_destructive()?;

    let target: String = target.chars().filter(|c| c.is_numeric()).collect();
    while let Some(version) = applied_migrations(conn)?.first() {
        if version.to_string() > target {
            let result = conn.raw_query_write(|conn| {
                Ok(conn
                    .revert_last_migration(xmtp_db::MIGRATIONS)
                    .map_err(DieselError::QueryBuilderError))
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

fn applied_migrations(conn: &impl ConnectionExt) -> Result<Vec<MigrationVersion<'static>>> {
    let applied_migrations: Vec<MigrationVersion<'static>> = conn.raw_query_read(|conn| {
        conn.applied_migrations()
            .map_err(DieselError::QueryBuilderError)
    })?;
    Ok(applied_migrations)
}
