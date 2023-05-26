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

use rand::RngCore;
use std::{ops::DerefMut, sync::Mutex};

use self::{models::*, schema::messages};
use crate::{Errorer, Fetch, Store};
use diesel::{connection::SimpleConnection, prelude::*, Connection};
use thiserror::Error;
use xmtp_cryptography::utils as crypto_utils;

use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations/");

#[derive(Debug, Error, PartialEq)]
pub enum EncryptedMessageStoreError {
    #[error("Diesel connection error")]
    DieselConnectError(#[from] diesel::ConnectionError),
    #[error("Diesel result error")]
    DieselResultError(#[from] diesel::result::Error),
    #[error("Either incorrect encryptionkey or file is not a db")]
    DbInitError,
    #[error("could not set encryption key")]
    EncryptionKey,
    #[error("could not generate encryptionKey")]
    EncryptionKeyGen,
    #[error("Mutex Poisioned: {0}")]
    PoisionError(String),
    #[error("unknown storage error")]
    Unknown,
}

pub type EncryptionKey = [u8; 32];

#[derive(Default)]
pub enum StorageOption {
    #[default]
    Ephemeral,
    Peristent(String),
}

#[allow(dead_code)]
/// Manages a Sqlite db for persisting messages and other objects.
pub struct EncryptedMessageStore {
    connect_opt: StorageOption,
    conn: Mutex<SqliteConnection>,
}
impl Errorer for EncryptedMessageStore {
    type Error = EncryptedMessageStoreError;
}

impl Default for EncryptedMessageStore {
    fn default() -> Self {
        Self::new(StorageOption::Ephemeral, Self::generate_enc_key())
            .expect("Error Occured: tring to create default Ephemeral store")
    }
}

impl EncryptedMessageStore {
    pub fn new(
        opts: StorageOption,
        enc_key: EncryptionKey,
    ) -> Result<Self, EncryptedMessageStoreError> {
        let db_path = match opts {
            StorageOption::Ephemeral => ":memory:",
            StorageOption::Peristent(ref path) => path,
        };

        let mut conn = SqliteConnection::establish(db_path)
            .map_err(EncryptedMessageStoreError::DieselConnectError)?;

        // Setup SqlCipherKey
        Self::set_sqlcipher_key(&mut conn, &enc_key)?;

        // TODO: Validate that sqlite is correctly configured. Bad EncKey is not detected until the migrations run which returns an
        // unhelpful error.

        let mut obj = Self {
            connect_opt: opts,
            conn: Mutex::new(conn),
        };

        obj.init_db()?;
        Ok(obj)
    }

    fn init_db(&mut self) -> Result<(), EncryptedMessageStoreError> {
        self.conn
            .lock()
            .map_err(|e| EncryptedMessageStoreError::PoisionError(e.to_string()))?
            .run_pending_migrations(MIGRATIONS)
            .map_err(|_| EncryptedMessageStoreError::DbInitError)?;
        Ok(())
    }

    pub fn create_fake_msg(&mut self, content: &str) {
        NewDecryptedMessage::new("convo".into(), "addr".into(), content.into())
            .store(self)
            .unwrap();
    }

    fn set_sqlcipher_key(
        conn: &mut SqliteConnection,
        encryption_key: &[u8; 32],
    ) -> Result<(), EncryptedMessageStoreError> {
        conn.batch_execute(&format!(
            "PRAGMA key = \"x'{}'\";",
            hex::encode(encryption_key)
        ))?;
        Ok(())
    }

    pub fn generate_enc_key() -> EncryptionKey {
        // TODO: Handle Key Better/ Zeroize
        let mut key = [0u8; 32];
        crypto_utils::rng().fill_bytes(&mut key[..]);
        key
    }
}

impl Store<EncryptedMessageStore> for NewDecryptedMessage {
    fn store(&self, into: &mut EncryptedMessageStore) -> Result<(), String> {
        let mut conn_guard = into.conn.lock().map_err(|e| e.to_string())?;
        diesel::insert_into(messages::table)
            .values(self)
            .execute(conn_guard.deref_mut())
            .expect("Error saving new message");

        Ok(())
    }
}

impl Fetch<DecryptedMessage> for EncryptedMessageStore {
    type E = EncryptedMessageStoreError;
    fn fetch(&mut self) -> Result<Vec<DecryptedMessage>, Self::E> {
        use self::schema::messages::dsl::*;
        let mut conn_guard = self
            .conn
            .lock()
            .map_err(|e| EncryptedMessageStoreError::PoisionError(e.to_string()))?;

        messages
            .load::<DecryptedMessage>(conn_guard.deref_mut())
            .map_err(EncryptedMessageStoreError::DieselResultError)
    }
}

#[cfg(test)]
mod tests {

    use super::{
        models::{DecryptedMessage, NewDecryptedMessage},
        EncryptedMessageStore, EncryptedMessageStoreError, StorageOption,
    };
    use crate::{Fetch, Store};
    use rand::{
        distributions::{Alphanumeric, DistString},
        Rng,
    };
    use std::fs;
    use std::{thread::sleep, time::Duration};

    fn rand_string() -> String {
        Alphanumeric.sample_string(&mut rand::thread_rng(), 16)
    }

    fn rand_vec() -> Vec<u8> {
        rand::thread_rng().gen::<[u8; 16]>().to_vec()
    }

    #[test]
    fn store_check() {
        let mut store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();

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
        let db_path = format!("{}.db3", rand_string());
        {
            let mut store = EncryptedMessageStore::new(
                StorageOption::Peristent(db_path.clone()),
                EncryptedMessageStore::generate_enc_key(),
            )
            .unwrap();

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
        let mut store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();

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

    #[test]
    fn keymismatch() {
        let mut enc_key = EncryptedMessageStore::generate_enc_key();

        let db_path = format!("{}.db3", rand_string());
        {
            // Setup a persistent store
            let mut store = EncryptedMessageStore::new(
                // StorageOption::Ephemeral,
                StorageOption::Peristent(db_path.clone()),
                enc_key.clone(),
            )
            .unwrap();

            let msg0 = NewDecryptedMessage::new(rand_string(), rand_string(), rand_vec());
            msg0.store(&mut store).unwrap();
        } // Drop it

        enc_key[3] = 145; // Alter the enc_key
        let res = EncryptedMessageStore::new(StorageOption::Peristent(db_path.clone()), enc_key);
        // Ensure it fails
        assert_eq!(res.err(), Some(EncryptedMessageStoreError::DbInitError));
        fs::remove_file(db_path).unwrap();
    }
}
