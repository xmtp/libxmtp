use crate::tasks::{DbBencher, db_vacuum, revert_migrations, run_pending_migrations};
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
                let Some(dest) = &args.dest else {
                    bail!(
                        "--dest argument must be provided for this task.\n\
                        This is where the persistent database will be written to.
                        "
                    );
                };
                db_vacuum(args.db, dest)?;
            }
            Task::DbRevert => {
                let Some(target) = &args.version else {
                    bail!(
                        "--version argument must be provided for this task.\n\
                        This is the target version you want to roll the database back to."
                    );
                };

                print!(
                    "Please confirm that you have backed up your database. This action can result in loss of data. (y/n): "
                );
                io::stdout().flush().unwrap();

                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();

                if matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
                    revert_migrations(&manager.store, target)?;
                }
            }
            Task::DbMigrate => {
                run_pending_migrations(&manager.store)?;
            }
        }

        info!("Finished {task:?}.");
    }

    Ok(())
}

#[derive(Parser)]
struct Args {
    /// Database path
    db: String,

    /// Destination path - used for some tasks
    dest: Option<String>,

    /// A hex encoded database encryption key
    #[arg(long)]
    db_key: Option<String>,

    /// Run a specific task
    #[arg(long, value_enum)]
    task: Option<Task>,

    #[arg(long)]
    version: Option<String>,
}

#[derive(ValueEnum, Clone, Debug)]
enum Task {
    /// Measure the performance of database queries
    /// to identify problematic performers.
    QueryBench,
    DbVacuum,
    DbRevert,
    DbMigrate,
}
