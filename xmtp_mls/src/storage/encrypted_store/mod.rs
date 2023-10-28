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

use crate::{Delete, Fetch, Store};

use self::{models::*, schema::*};

use super::StorageError;
use diesel::{
    connection::SimpleConnection,
    prelude::*,
    r2d2::{ConnectionManager, Pool, PooledConnection},
    result::DatabaseErrorKind,
    result::Error,
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use log::warn;
use rand::RngCore;
use xmtp_cryptography::utils as crypto_utils;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations/");

pub type DbConnection = PooledConnection<ConnectionManager<SqliteConnection>>;

pub type EncryptionKey = [u8; 32];

#[derive(Default, Clone, Debug)]
pub enum StorageOption {
    #[default]
    Ephemeral,
    Persistent(String),
}

#[allow(dead_code)]
pub fn ignore_unique_violation<T>(
    result: Result<T, diesel::result::Error>,
) -> Result<(), StorageError> {
    match result {
        Ok(_) => Ok(()),
        Err(Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => Ok(()),
        Err(error) => Err(StorageError::from(error)),
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
/// Manages a Sqlite db for persisting messages and other objects.
pub struct EncryptedMessageStore {
    connect_opt: StorageOption,
    pool: Pool<ConnectionManager<SqliteConnection>>,
}

impl Default for EncryptedMessageStore {
    fn default() -> Self {
        Self::new(StorageOption::Ephemeral, Self::generate_enc_key())
            .expect("Error Occured: tring to create default Ephemeral store")
    }
}

impl EncryptedMessageStore {
    pub fn new(opts: StorageOption, enc_key: EncryptionKey) -> Result<Self, StorageError> {
        Self::new_database(opts, Some(enc_key))
    }

    pub fn new_unencrypted(opts: StorageOption) -> Result<Self, StorageError> {
        Self::new_database(opts, None)
    }

    /// This function is private so that an unencrypted database cannot be created by accident
    fn new_database(
        opts: StorageOption,
        enc_key: Option<EncryptionKey>,
    ) -> Result<Self, StorageError> {
        let pool = match opts {
            StorageOption::Ephemeral => Pool::builder()
                .max_size(1)
                .build(ConnectionManager::<SqliteConnection>::new(":memory:"))
                .map_err(|e| StorageError::DbInit(e.to_string()))?,
            StorageOption::Persistent(ref path) => Pool::builder()
                .max_size(10)
                .build(ConnectionManager::<SqliteConnection>::new(path))
                .map_err(|e| StorageError::DbInit(e.to_string()))?,
        };

        // // Setup SqlCipherKey
        if let Some(key) = enc_key {
            Self::set_sqlcipher_key(pool.clone(), &key)?;
        }

        // TODO: Validate that sqlite is correctly configured. Bad EncKey is not detected until the migrations run which returns an
        // unhelpful error.

        let mut obj = Self {
            connect_opt: opts,
            pool,
        };

        obj.init_db()?;
        Ok(obj)
    }

    fn init_db(&mut self) -> Result<(), StorageError> {
        let conn = &mut self.conn()?;

        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| StorageError::DbInit(e.to_string()))?;

        Ok(())
    }

    pub fn conn(
        &self,
    ) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, StorageError> {
        let conn = self
            .pool
            .get()
            .map_err(|e| StorageError::Pool(e.to_string()))?;

        Ok(conn)
    }

    fn set_sqlcipher_key(
        pool: Pool<ConnectionManager<SqliteConnection>>,
        encryption_key: &[u8; 32],
    ) -> Result<(), StorageError> {
        let conn = &mut pool.get().map_err(|e| StorageError::Pool(e.to_string()))?;

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

#[allow(dead_code)]
fn warn_length<T>(list: &Vec<T>, str_id: &str, max_length: usize) {
    if list.len() > max_length {
        warn!(
            "EncryptedStore expected at most {} {} however found {}. Using the Oldest.",
            max_length,
            str_id,
            list.len()
        )
    }
}

impl Store<DbConnection> for StoredKeyStoreEntry {
    fn store(&self, into: &mut DbConnection) -> Result<(), StorageError> {
        diesel::insert_into(openmls_key_store::table)
            .values(self)
            .execute(into)?;

        Ok(())
    }
}

impl Fetch<StoredKeyStoreEntry> for DbConnection {
    type Key = Vec<u8>;
    fn fetch(&mut self, key: Vec<u8>) -> Result<Option<StoredKeyStoreEntry>, StorageError> where {
        use self::schema::openmls_key_store::dsl::*;
        Ok(openmls_key_store.find(key).first(self).optional()?)
    }
}

impl Delete<StoredKeyStoreEntry> for DbConnection {
    type Key = Vec<u8>;
    fn delete(&mut self, key: Vec<u8>) -> Result<usize, StorageError> where {
        use self::schema::openmls_key_store::dsl::*;
        Ok(diesel::delete(openmls_key_store.filter(key_bytes.eq(key))).execute(self)?)
    }
}

impl Store<DbConnection> for StoredIdentity {
    fn store(&self, into: &mut DbConnection) -> Result<(), StorageError> {
        diesel::insert_into(identity::table)
            .values(self)
            .execute(into)?;
        Ok(())
    }
}

impl Fetch<StoredIdentity> for DbConnection {
    type Key = ();
    fn fetch(&mut self, _key: ()) -> Result<Option<StoredIdentity>, StorageError> where {
        use self::schema::identity::dsl::*;
        Ok(identity.first(self).optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::{models::*, EncryptedMessageStore, StorageError, StorageOption};
    use crate::{Fetch, Store};
    use rand::{
        distributions::{Alphanumeric, DistString},
        Rng,
    };
    use std::boxed::Box;
    use std::fs;

    fn rand_string() -> String {
        Alphanumeric.sample_string(&mut rand::thread_rng(), 16)
    }

    fn rand_vec() -> Vec<u8> {
        rand::thread_rng().gen::<[u8; 16]>().to_vec()
    }

    #[test]
    fn ephemeral_store() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();
        let conn = &mut store.conn().unwrap();

        let account_address = "address";
        StoredIdentity::new(account_address.to_string(), rand_vec(), rand_vec())
            .store(conn)
            .unwrap();

        let fetched_identity: StoredIdentity = conn.fetch(()).unwrap().unwrap();
        assert_eq!(fetched_identity.account_address, account_address);
    }

    #[test]
    fn persistent_store() {
        let db_path = format!("{}.db3", rand_string());
        {
            let store = EncryptedMessageStore::new(
                StorageOption::Persistent(db_path.clone()),
                EncryptedMessageStore::generate_enc_key(),
            )
            .unwrap();
            let conn = &mut store.conn().unwrap();

            let account_address = "address";
            StoredIdentity::new(account_address.to_string(), rand_vec(), rand_vec())
                .store(conn)
                .unwrap();

            let fetched_identity: StoredIdentity = conn.fetch(()).unwrap().unwrap();
            assert_eq!(fetched_identity.account_address, account_address);
        }

        fs::remove_file(db_path).unwrap();
    }

    #[test]
    fn mismatched_encryption_key() {
        let mut enc_key = [1u8; 32];

        let db_path = format!("{}.db3", rand_string());
        {
            // Setup a persistent store
            let store = EncryptedMessageStore::new(
                StorageOption::Persistent(db_path.clone()),
                enc_key.clone(),
            )
            .unwrap();

            StoredIdentity::new("dummy_address".to_string(), rand_vec(), rand_vec())
                .store(&mut store.conn().unwrap())
                .unwrap();
        } // Drop it

        enc_key[3] = 145; // Alter the enc_key
        let res = EncryptedMessageStore::new(StorageOption::Persistent(db_path.clone()), enc_key);
        // Ensure it fails
        assert!(
            matches!(res.err(), Some(StorageError::DbInit(_))),
            "Expected DbInitError"
        );
        fs::remove_file(db_path).unwrap();
    }

    #[test]
    fn can_only_store_one_identity() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();
        let conn = &mut store.conn().unwrap();

        StoredIdentity::new("".to_string(), rand_vec(), rand_vec())
            .store(conn)
            .unwrap();

        let duplicate_insertion =
            StoredIdentity::new("".to_string(), rand_vec(), rand_vec()).store(conn);
        assert!(duplicate_insertion.is_err());
    }

    #[test]
    fn it_returns_ok_when_given_ok_result() {
        let result: Result<(), diesel::result::Error> = Ok(());
        assert!(
            super::ignore_unique_violation(result).is_ok(),
            "Expected Ok(()) when given Ok result"
        );
    }

    #[test]
    fn it_returns_ok_on_unique_violation_error() {
        let result: Result<(), diesel::result::Error> = Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            Box::new("violation".to_string()),
        ));
        assert!(
            super::ignore_unique_violation(result).is_ok(),
            "Expected Ok(()) when given UniqueViolation error"
        );
    }

    #[test]
    fn it_returns_err_on_non_unique_violation_database_errors() {
        let result: Result<(), diesel::result::Error> = Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::NotNullViolation,
            Box::new("other kind".to_string()),
        ));
        assert!(
            super::ignore_unique_violation(result).is_err(),
            "Expected Err when given non-UniqueViolation database error"
        );
    }

    #[test]
    fn it_returns_err_on_non_database_errors() {
        let result: Result<(), diesel::result::Error> = Err(diesel::result::Error::NotFound);
        assert!(
            super::ignore_unique_violation(result).is_err(),
            "Expected Err when given a non-database error"
        );
    }
}
