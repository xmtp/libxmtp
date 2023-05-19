pub mod models;
pub mod schema;

use self::{models::*, schema::messages};
use crate::{Errorer, Fetch, Store};
use diesel::{prelude::*, Connection};
use thiserror::Error;

use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./diesel/migrations/");

#[derive(Debug, Error)]
pub enum UnencryptedMessageStoreError {
    #[error("Diesel connection error")]
    DieselConnectError(#[from] diesel::ConnectionError),
    #[error("Diesel result error")]
    DieselResultError(#[from] diesel::result::Error),
    #[error("Diesel migration error")]
    DbMigrationError(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("unknown storage error")]
    Unknown,
}

pub enum StorageOption {
    Ephemeral,
    Peristent(String),
}

impl Default for StorageOption {
    fn default() -> Self {
        StorageOption::Ephemeral
    }
}

#[allow(dead_code)]
pub struct UnencryptedMessageStore {
    connect_opt: StorageOption,
    conn: SqliteConnection,
}
impl Errorer for UnencryptedMessageStore {
    type Error = UnencryptedMessageStoreError;
}

impl Default for UnencryptedMessageStore {
    fn default() -> Self {
        Self::new(StorageOption::Ephemeral)
            .expect("Error Occured: tring to create default Ephemeral store")
    }
}

impl UnencryptedMessageStore {
    pub fn new(opts: StorageOption) -> Result<Self, UnencryptedMessageStoreError> {
        let db_path = match opts {
            StorageOption::Ephemeral => ":memory:",
            StorageOption::Peristent(ref path) => path,
        };

        let conn = SqliteConnection::establish(db_path)
            .map_err(UnencryptedMessageStoreError::DieselConnectError)?;
        let mut obj = Self {
            connect_opt: opts,
            conn,
        };

        obj.init_db()?;
        Ok(obj)
    }

    fn init_db(&mut self) -> Result<(), UnencryptedMessageStoreError> {
        self.conn.run_pending_migrations(MIGRATIONS)?;
        Ok(())
    }
}

impl Store<UnencryptedMessageStore> for NewDecryptedMessage {
    fn store(&self, into: &mut UnencryptedMessageStore) -> Result<(), String> {
        diesel::insert_into(messages::table)
            .values(self)
            .execute(&mut into.conn)
            .expect("Error saving new message");

        Ok(())
    }
}

// TODO: Add Filtering to Query
impl Fetch<DecryptedMessage> for UnencryptedMessageStore {
    type E = UnencryptedMessageStoreError;
    fn fetch(&mut self) -> Result<Vec<DecryptedMessage>, Self::E> {
        use self::schema::messages::dsl::*;
        messages
            .limit(5)
            .load::<DecryptedMessage>(&mut self.conn)
            .map_err(UnencryptedMessageStoreError::DieselResultError)
    }
}

#[cfg(test)]
mod tests {

    use crate::{Fetch, Store};

    use super::{
        models::{DecryptedMessage, NewDecryptedMessage},
        StorageOption, UnencryptedMessageStore,
    };

    #[test]
    fn store_check() {
        let mut store = UnencryptedMessageStore::new(StorageOption::Ephemeral).unwrap();

        NewDecryptedMessage::new("Bola".into(), "0x000A".into(), "Hello Bola".into())
            .store(&mut store)
            .unwrap();

        NewDecryptedMessage::new("Mark".into(), "0x000A".into(), "Sup Mark".into())
            .store(&mut store)
            .unwrap();

        NewDecryptedMessage::new("Bola".into(), "0x000B".into(), "Hey Amal".into())
            .store(&mut store)
            .unwrap();

        NewDecryptedMessage::new("Bola".into(), "0x000A".into(), "bye".into())
            .store(&mut store)
            .unwrap();

        let v: Vec<DecryptedMessage> = store.fetch().unwrap();
        assert_eq!(4, v.len());
    }

    #[test]
    fn store_persistent() {
        use rand::distributions::{Alphanumeric, DistString};
        use std::fs;
        let dbname = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
        let db_path = format!("{}.db3", dbname);
        {
            let mut store =
                UnencryptedMessageStore::new(StorageOption::Peristent(db_path.clone())).unwrap();

            NewDecryptedMessage::new("Bola".into(), "0x000A".into(), "Hello Bola".into())
                .store(&mut store)
                .unwrap();

            let v: Vec<DecryptedMessage> = store.fetch().unwrap();
            assert_eq!(1, v.len());
        }

        fs::remove_file(db_path).unwrap();
    }

    // TODO: Add Content Tests
}
