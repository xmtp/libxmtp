use crate::tasks::DbBencher;
use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};
use dotenvy::{dotenv, var};
use std::io::{self, Write};
use tracing::info;
use xmtp_db::{
    ConnectionExt, DbConnection, EncryptedMessageStore, NativeDb, StorageOption, XmtpDb,
    migrations::QueryMigrations,
};

mod tasks;

struct Manager<Db> {
    store: Db,
}

impl<Db> Manager<Db>
where
    Db: XmtpDb + Clone,
    <Db as XmtpDb>::Connection: ConnectionExt,
{
    fn new_bencher(&self) -> Result<DbBencher<Db>> {
        DbBencher::new(self.store.clone())
    }
}

const ENV_ENC_KEY: &str = "XMTP_DB_ENCRYPTION_KEY";

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .init();

    let _ = dotenv();
    let args = Args::parse().load_env();

    tracing::info!("Starting up.");

    // Connect to the database
    let storage_option = StorageOption::Persistent(args.db.clone());
    let db_key = std::env::var(ENV_ENC_KEY)
        .ok()
        .or_else(|| std::env::var("ENCRYPTION_KEY").ok())
        .or_else(|| args.db_key.clone());

    let db = match &db_key {
        Some(key) => {
            tracing::info!("Db Key: \"{}...\"", &key[..4]);
            let key_bytes = hex::decode(key)?;
            if key_bytes.len() != 32 {
                bail!("Encryption key must be exactly 32 bytes (64 hex characters)");
            }
            NativeDb::new(&storage_option, key_bytes.try_into().unwrap())
        }
        None => {
            tracing::info!("No db encryption key provided.");
            NativeDb::new_unencrypted(&storage_option)
        }
    }?;
    let store = EncryptedMessageStore::new(db)?;

    let manager = Manager { store };

    match &args.task {
        Task::QueryBench => {
            tracing::info!("Running query bench task.");
            manager.new_bencher()?.bench()?;
        }
        Task::DbVacuum => {
            let target =
                args.target("This will be where the persistent database will be written to.");
            tasks::db_vacuum(&manager.store, target)?;
        }
        Task::DbRollback => {
            let target = args
                .target("This will be the target version you want to roll the database back to.");
            tasks::rollback(&manager.store.conn(), target)?;
        }
        Task::DbClearAllMessages => {
            tasks::clear_all_messages(&manager.store.conn(), args.retain_days, None)?;
        }
        Task::DbClearMessages => {
            let group_ids = args.group_ids()?;
            tasks::clear_all_messages(&manager.store.conn(), args.retain_days, Some(&group_ids))?;
        }
        Task::DbRunMigration => {
            let target = args.target(
                "This will be the name of the target migration you wish to run on the database.",
            );
            tasks::run_migration(&manager.store.conn(), target)?;
        }
        Task::DbRevertMigration => {
            let target = args.target(
                "This will be the name of the target migration you wish to run on the database.",
            );
            tasks::revert_migration(&manager.store.conn(), target)?;
        }
        Task::EnableGroup => {
            let arg_group_ids = args.group_ids()?;
            let group_ids: Vec<_> = arg_group_ids.iter().map(Vec::as_slice).collect();
            tasks::enable_groups(&manager.store.db(), &group_ids)?;
        }
        Task::DisableGroup => {
            let arg_group_ids = args.group_ids()?;
            let group_ids: Vec<_> = arg_group_ids.iter().map(Vec::as_slice).collect();
            tasks::disable_groups(&manager.store.db(), &group_ids)?;
        }
        Task::DbListMigrations => {
            let conn = manager.store.conn();
            let db = DbConnection::new(&conn);
            let mut available = db.available_migrations()?;
            let applied = db.applied_migrations()?;

            // Sort by date descending (most recent first)
            available.sort_by(|a, b| b.cmp(a));

            println!("Available migrations ({}):", available.len());
            for name in &available {
                let name_version: String = name.chars().filter(|c| c.is_numeric()).collect();
                let status = if applied.iter().any(|a| {
                    let applied_version: String = a.chars().filter(|c| c.is_numeric()).collect();
                    name_version == applied_version
                }) {
                    "[applied]"
                } else {
                    "[pending]"
                };
                println!("  {status} {name}");
            }
        }
    }

    Ok(())
}

fn confirm_destructive() -> Result<()> {
    print!(
        "Please confirm that you have backed up your database. This action can result in loss of data. (y/n): "
    );
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
        bail!("Operation cancelled");
    }

    Ok(())
}

#[derive(Parser)]
struct Args {
    /// Run a specific task
    #[arg(value_enum)]
    task: Task,

    /// Database path
    db: String,

    /// Target - purpose varies by task
    #[arg(long, short)]
    target: Option<String>,

    /// A hex encoded database encryption key
    #[arg(long)]
    db_key: Option<String>,

    /// Number of days worth of data you'd like to retain on delete tasks.
    #[arg(long, value_parser = clap::value_parser!(u32).range(0..))]
    retain_days: Option<u32>,
}

impl Args {
    fn load_env(mut self) -> Self {
        if self.db_key.is_none()
            && let Ok(key) = var("XMTP_DB_ENCRYPTION_KEY")
        {
            info!("Loading database encryption key from .env file.");
            self.db_key = Some(key);
        }

        self
    }

    fn target(&self, reason: &str) -> &str {
        self.target.as_ref().unwrap_or_else(|| {
            panic!("--target argument must be provided for this task.\n {reason}")
        })
    }

    fn group_ids(&self) -> Result<Vec<Vec<u8>>> {
        let Some(group_id) = &self.target else {
            bail!("A hex-encoded group_id must be provided as the --target param for this task.");
        };
        let group_ids: Vec<Vec<u8>> = group_id
            .split(',')
            .filter(|id| !id.trim().is_empty())
            .map(hex::decode)
            .collect::<Result<Vec<_>, _>>()?;

        if group_ids.is_empty() {
            bail!("At least one group_id must be provided.");
        }

        Ok(group_ids)
    }
}

#[derive(ValueEnum, Clone, Debug)]
enum Task {
    /// Measure the performance of database queries
    /// to identify problematic performers.
    QueryBench,
    /// Dump an encrypted database into an un-encrypted file.
    DbVacuum,
    /// Attempt to revert all migrations after and including specified migration version.
    /// Requires migration name as --target param.
    DbRollback,
    /// Attempt to run a specific migration.
    /// Requires migration name as --target param.
    DbRunMigration,
    /// Attempt to revert a specific migration.
    /// Requires migration name as --target param.
    DbRevertMigration,
    /// Clear all messages in a group.
    /// Requires hex-encoded group_id as --target param.
    DbClearMessages,
    /// Clear all messages in the database.
    DbClearAllMessages,
    /// Disable a group.
    /// Requires hex-encoded group_id as --target param.
    DisableGroup,
    /// Enable a group.
    /// Requires hex-encoded group_id as --target param.
    EnableGroup,
    /// List all available migrations and their status.
    DbListMigrations,
}
