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

fn main() -> Result<()> {
    let args = Args::parse();

    // Connect to the database
    let storage_option = StorageOption::Persistent(args.db.clone());
    // let storage_option = StorageOption::Ephemeral;
    let db = match &args.db_key {
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
                tasks::clear_all_messages(&manager.store)?;
            }
            Task::DbClearMessages => {
                let Some(group_id) = &args.target else {
                    bail!(
                        "A hex-encoded group_id must be provided as the --target param for this task."
                    );
                };

                let group_id = hex::decode(group_id)?;

                tasks::clear_all_messages_for_group(&manager.store, &group_id)?;
            }
        }

        info!("Finished {task:?}.");
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
    /// Clear all messages in the database.
    DbClearMessages,
    /// Clear all messages in a group.
    /// Requirese hex-encoded group_id as --target param.
    DbClearAllMessages,
}
