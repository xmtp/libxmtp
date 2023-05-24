//! A durable object store powered by Sqlite and Diesel.
//!
//! Provides mechanism to store objects between sessions. The behavor of the store can be tailored by
//! choosing an appropriate `StoreOption`.
//!
//! ## Migrations
//!
//! Table definitions are located `<PacakgeRoot>/migrations/`. On intialization the store will see if
//! there are any outstanding database migrations and perform them as needed. When updating the table
//! definitions `schema.rs` must also be updated. To generate the correct schemas you can run
//! `diesel print-schema` or use `cargo run update-schema` which will update the files for you.      
//!

pub mod models;
pub mod schema;

use std::{
    ops::{Deref, DerefMut},
    sync::Mutex,
};

use self::{models::*, schema::messages};
use crate::{Errorer, Fetch, Store};
use diesel::{prelude::*, Connection};
use thiserror::Error;

use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations/");

#[derive(Debug, Error)]
pub enum UnencryptedMessageStoreError {
    #[error("Diesel connection error")]
    DieselConnectError(#[from] diesel::ConnectionError),
    #[error("Diesel result error")]
    DieselResultError(#[from] diesel::result::Error),
    #[error("Diesel migration error")]
    DbMigrationError(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("Mutex Poisioned: {0}")]
    PoisionError(String),
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
/// Manages a Sqlite db for persisting messages and other objects.
pub struct UnencryptedMessageStore {
    connect_opt: StorageOption,
    conn: Mutex<SqliteConnection>,
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
            conn: Mutex::new(conn),
        };

        obj.init_db()?;
        Ok(obj)
    }

    fn init_db(&mut self) -> Result<(), UnencryptedMessageStoreError> {
        self.conn
            .lock()
            .map_err(|e| UnencryptedMessageStoreError::PoisionError(e.to_string()))?
            .run_pending_migrations(MIGRATIONS)?;
        Ok(())
    }
}

impl Store<UnencryptedMessageStore> for NewDecryptedMessage {
    fn store(&self, into: &mut UnencryptedMessageStore) -> Result<(), String> {
        let mut conn_guard = into.conn.lock().map_err(|e| e.to_string())?;
        diesel::insert_into(messages::table)
            .values(self)
            .execute(conn_guard.deref_mut())
            .expect("Error saving new message");

        Ok(())
    }
}

impl Fetch<DecryptedMessage> for UnencryptedMessageStore {
    type E = UnencryptedMessageStoreError;
    fn fetch(&mut self) -> Result<Vec<DecryptedMessage>, Self::E> {
        use self::schema::messages::dsl::*;
        let mut conn_guard = self
            .conn
            .lock()
            .map_err(|e| UnencryptedMessageStoreError::PoisionError(e.to_string()))?;

        messages
            .load::<DecryptedMessage>(conn_guard.deref_mut())
            .map_err(UnencryptedMessageStoreError::DieselResultError)
    }
}

#[cfg(test)]
mod tests {

    use super::{
        models::{DecryptedMessage, NewDecryptedMessage},
        StorageOption, UnencryptedMessageStore,
    };
    use crate::{Fetch, Store};
    use rand::{
        distributions::{Alphanumeric, DistString},
        Rng,
    };
    use std::{thread::sleep, time::Duration};

    fn rand_string() -> String {
        Alphanumeric.sample_string(&mut rand::thread_rng(), 16)
    }

    fn rand_vec() -> Vec<u8> {
        rand::thread_rng().gen::<[u8; 16]>().to_vec()
    }

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
        use std::fs;

        let db_path = format!("{}.db3", rand_string());
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

    #[test]
    fn message_roundtrip() {
        let mut store = UnencryptedMessageStore::new(StorageOption::Ephemeral).unwrap();

        let msg0 = NewDecryptedMessage::new(rand_string(), rand_string(), rand_vec());
        sleep(Duration::from_millis(10));
        let msg1 = NewDecryptedMessage::new(rand_string(), rand_string(), rand_vec());

        msg0.store(&mut store).unwrap();
        msg1.store(&mut store).unwrap();

        let msgs = store.fetch().unwrap();

        assert_eq!(2, msgs.len());
        assert_eq!(msg0, msgs[0]);
        assert_eq!(msg1, msgs[1]);
        assert!(msgs[1].created_at > msgs[0].created_at);
    }
}
