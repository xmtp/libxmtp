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

use crate::confirm_destructive;

pub fn revert_migrations(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    confirm_destructive()?;
    revert_migrations_confirmed(conn, target)
}

pub fn revert_migrations_confirmed(conn: &impl ConnectionExt, target: &str) -> Result<()> {
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

pub fn run_migration(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    confirm_destructive()?;
    run_migration_confirmed(conn, target)
}

fn run_migration_confirmed(conn: &impl ConnectionExt, target: &str) -> Result<()> {
    let migrations: Vec<Box<dyn Migration<Sqlite>>> = xmtp_db::MIGRATIONS.migrations().unwrap();

    println!("?");
    tracing::info!("?");

    for migration in migrations {
        if migration.name().to_string() != target {
            continue;
        }

        info!("Running migration for {target}...");

        conn.raw_query_write(|c| {
            c.run_migration(&migration).unwrap();
            Ok(())
        })?;
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

    use crate::tasks::revert_migrations_confirmed;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_revert_migrations_and_back() {
        tester!(alix);
        tester!(bo);

        let (dm, _) = alix.test_talk_in_dm_with(&bo).await?;

        let bo_dm = bo.group(&dm.group_id)?;
        let bo_msgs = bo_dm.find_messages(&Default::default())?;
        assert_eq!(bo_msgs.len(), 2);

        // Go back and forth a couple times
        for _ in 0..2 {
            revert_migrations_confirmed(&alix.db(), "2025-07-08-010431_modify_commit_log")?;
            alix.db().raw_query_write(|c| {
                c.run_pending_migrations(xmtp_db::MIGRATIONS)?;
                Ok(())
            })?;
        }

        alix.test_talk_in_dm_with(&bo).await?;
    }
}
