use crate::tasks::DbBencher;
use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};
use std::io::{self, Write};
use tracing::info;
use xmtp_db::{EncryptedMessageStore, NativeDb, StorageOption};

mod tasks;

struct Manager {
    store: EncryptedMessageStore<NativeDb>,
}

impl Manager {
    fn new_bencher(&self) -> Result<DbBencher> {
        DbBencher::new(self.store.clone())
    }
}

const ENV_ENC_KEY: &str = "XMTP_DB_ENCRYPTION_KEY";

fn main() -> Result<()> {
    let args = Args::parse();

    // Connect to the database
    let storage_option = StorageOption::Persistent(args.db.clone());
    let db_key = std::env::var(ENV_ENC_KEY)
        .ok()
        .or_else(|| args.db_key.clone());

    // let storage_option = StorageOption::Ephemeral;
    let db = match &db_key {
        Some(key) => {
            let key = hex::decode(key)?;
            NativeDb::new(&storage_option, key.try_into().unwrap())
        }
        None => NativeDb::new_unencrypted(&storage_option),
    }?;
    let store = EncryptedMessageStore::new(db)?;

    let manager = Manager { store };

    if let Some(task) = &args.task {
        match task {
            Task::QueryBench => {
                manager.new_bencher()?.bench()?;
            }
            Task::DbVacuum => {
                let Some(dest) = &args.target else {
                    bail!(
                        "dest argument must be provided for this task.\n\
                        This is where the persistent database will be written to.
                        "
                    );
                };
                tasks::db_vacuum(&manager.store, dest)?;
            }
            Task::DbRevert => {
                let Some(target) = &args.target else {
                    bail!(
                        "--version argument must be provided for this task.\n\
                        This is the target version you want to roll the database back to."
                    );
                };

                tasks::revert_migrations(&manager.store, target)?;
            }
            Task::DbClearAllMessages => {
                tasks::clear_all_messages(&manager.store, args.retain_days)?;
            }
            Task::DbClearMessages => {
                tasks::clear_all_messages_for_groups(
                    &manager.store,
                    &args.group_ids()?,
                    args.retain_days,
                )?;
            }
            Task::EnableGroup => {
                tasks::enable_groups(&manager.store, &args.group_ids()?)?;
            }
            Task::DisableGroup => {
                tasks::disable_groups(&manager.store, &args.group_ids()?)?;
            }
        }

        info!("Finished {task:?}.");
    }

    Ok(())
}

impl Args {
    fn group_ids(&self) -> Result<Vec<Vec<u8>>> {
        let Some(group_id) = &self.target else {
            bail!("A hex-encoded group_id must be provided as the --target param for this task.");
        };
        let group_ids = group_id
            .split(",")
            .into_iter()
            .filter(|id| !id.trim().is_empty())
            .map(hex::decode)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(group_ids)
    }
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
    /// Database path
    db: String,

    /// Target - purpose varies by task
    #[arg(long)]
    target: Option<String>,

    /// A hex encoded database encryption key
    #[arg(long)]
    db_key: Option<String>,

    /// Run a specific task
    #[arg(long, value_enum)]
    task: Option<Task>,

    /// Number of days worth of data you'd like to retain on delete tasks.
    #[arg(long, value_enum)]
    retain_days: Option<i64>,
}

#[derive(ValueEnum, Clone, Debug)]
enum Task {
    /// Measure the performance of database queries
    /// to identify problematic performers.
    QueryBench,
    /// Dump an encrypted database into an un-encrypted file.
    DbVacuum,
    /// Attempt to revert database to a specific db version.
    /// Requires migration name as --target param.
    DbRevert,
    /// Clear all messages in a group.
    DbClearMessages,
    /// Clear all messages in the database.
    /// Requirese hex-encoded group_id as --target param.
    DbClearAllMessages,
    /// Disable a group.
    /// Requirese hex-encoded group_id as --target param.
    DisableGroup,
    /// Enable a group.
    /// Requirese hex-encoded group_id as --target param.
    EnableGroup,
}
