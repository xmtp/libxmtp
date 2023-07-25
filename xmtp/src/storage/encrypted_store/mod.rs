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

use self::{
    models::*,
    schema::{accounts, conversations, installations, messages, users},
};
use super::StorageError;
use crate::{account::Account, Errorer, Fetch, Store};
use diesel::{
    connection::SimpleConnection,
    prelude::*,
    r2d2::{ConnectionManager, Pool, PooledConnection},
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use log::warn;
use rand::RngCore;
use xmtp_cryptography::utils as crypto_utils;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations/");
pub type EncryptionKey = [u8; 32];

#[derive(Default, Clone, Debug)]
pub enum StorageOption {
    #[default]
    Ephemeral,
    Persistent(String),
}

#[allow(dead_code)]
#[derive(Clone)]
/// Manages a Sqlite db for persisting messages and other objects.
pub struct EncryptedMessageStore {
    connect_opt: StorageOption,
    pool: Pool<ConnectionManager<SqliteConnection>>,
}

impl Errorer for EncryptedMessageStore {
    type Error = StorageError;
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
        let db_path = match opts {
            StorageOption::Ephemeral => ":memory:",
            StorageOption::Persistent(ref path) => path,
        };

        let pool = Pool::builder()
            .max_size(1)
            .build(ConnectionManager::<SqliteConnection>::new(db_path))
            .map_err(|e| StorageError::DbInitError(e.to_string()))?;

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
            .map_err(|e| StorageError::DbInitError(e.to_string()))?;

        Ok(())
    }

    pub fn create_fake_msg(&self, content: &str, state: i32) {
        NewDecryptedMessage::new("convo".into(), "addr".into(), content.into(), state)
            .store(self)
            .unwrap();
    }

    pub fn conn(
        &self,
    ) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, StorageError> {
        let conn = self
            .pool
            .get()
            .map_err(|e| StorageError::PoolError(e.to_string()))?;

        Ok(conn)
    }

    fn set_sqlcipher_key(
        pool: Pool<ConnectionManager<SqliteConnection>>,
        encryption_key: &[u8; 32],
    ) -> Result<(), StorageError> {
        let conn = &mut pool
            .get()
            .map_err(|e| StorageError::PoolError(e.to_string()))?;

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

    pub fn get_account(&mut self) -> Result<Option<Account>, StorageError> {
        let mut account_list: Vec<Account> = self.fetch()?;

        warn_length(&account_list, "StoredAccount", 1);

        Ok(account_list.pop())
    }

    pub fn get_session(
        &self,
        installation_id: &str,
    ) -> Result<Option<StoredSession>, StorageError> {
        let conn = &mut self.conn()?;
        use self::schema::sessions::dsl::*;

        let mut session_list = sessions
            .filter(peer_installation_id.eq(installation_id))
            .order(created_at.desc())
            .load::<StoredSession>(conn)
            .map_err(|e| StorageError::Unknown(e.to_string()))?;

        warn_length(&session_list, "StoredSession", 1);
        Ok(session_list.pop())
    }

    pub fn get_sessions(&self, user_address: &str) -> Result<Vec<StoredSession>, StorageError> {
        let conn = &mut self.conn()?;
        use self::schema::sessions::dsl as schema;

        let session_list = schema::sessions
            .filter(schema::user_address.eq(user_address))
            .order(schema::created_at.desc())
            .load::<StoredSession>(conn)
            .map_err(|e| StorageError::Unknown(e.to_string()))?;
        Ok(session_list)
    }

    pub fn get_installations(
        &self,
        user_address_str: &str,
    ) -> Result<Vec<StoredInstallation>, StorageError> {
        use self::schema::installations::dsl as schema;
        let conn = &mut self.conn()?;

        let installation_list = schema::installations
            .filter(schema::user_address.eq(user_address_str))
            .load::<StoredInstallation>(conn)
            .map_err(|e| StorageError::Unknown(e.to_string()))?;

        Ok(installation_list)
    }

    pub fn get_user(&self, address: &str) -> Result<Option<StoredUser>, StorageError> {
        let conn = &mut self.conn()?;

        let mut user_list = users::table
            .filter(users::user_address.eq(address))
            .load::<StoredUser>(conn)?;

        warn_length(&user_list, "StoredUser", 1);
        Ok(user_list.pop())
    }

    pub fn insert_or_ignore_user(&self, user: StoredUser) -> Result<(), StorageError> {
        let conn = &mut self.conn()?;
        diesel::insert_or_ignore_into(users::table)
            .values(user)
            .execute(conn)?;
        Ok(())
    }

    pub fn get_conversation(
        &self,
        convo_id: &str,
    ) -> Result<Option<StoredConversation>, StorageError> {
        let conn = &mut self.conn()?;

        let mut convo_list = conversations::table
            .find(convo_id)
            .load::<StoredConversation>(conn)?;

        warn_length(&convo_list, "StoredConversation", 1);
        Ok(convo_list.pop())
    }

    pub fn insert_or_ignore_conversation(
        &self,
        conversation: StoredConversation,
    ) -> Result<(), StorageError> {
        let conn = &mut self.conn()?;
        diesel::insert_or_ignore_into(schema::conversations::table)
            .values(conversation)
            .execute(conn)?;
        Ok(())
    }

    pub fn get_contacts(
        &self,
        user_address: &str,
    ) -> Result<Vec<StoredInstallation>, StorageError> {
        let conn = &mut self.conn()?;
        use self::schema::installations::dsl;

        let install_list = dsl::installations
            .filter(dsl::user_address.eq(user_address))
            .order(dsl::first_seen_ns.desc())
            .load::<StoredInstallation>(conn)
            .map_err(|e| StorageError::Unknown(e.to_string()))?;

        Ok(install_list)
    }

    pub fn insert_or_ignore_install(
        &self,
        install: StoredInstallation,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), StorageError> {
        diesel::insert_or_ignore_into(installations::table)
            .values(install)
            .execute(conn)?;
        Ok(())
    }

    pub fn insert_or_ignore_session(
        &self,
        session: StoredSession,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), StorageError> {
        diesel::insert_or_ignore_into(schema::sessions::table)
            .values(session)
            .execute(conn)?;
        Ok(())
    }
}

impl Store<EncryptedMessageStore> for NewDecryptedMessage {
    fn store(&self, into: &EncryptedMessageStore) -> Result<(), StorageError> {
        let conn = &mut into.conn()?;
        diesel::insert_into(messages::table)
            .values(self)
            .execute(conn)?;

        Ok(())
    }
}

impl Store<EncryptedMessageStore> for StoredSession {
    fn store(&self, into: &EncryptedMessageStore) -> Result<(), StorageError> {
        let conn = &mut into.conn()?;
        diesel::insert_into(schema::sessions::table)
            .values(self)
            .execute(conn)?;

        Ok(())
    }
}

impl Fetch<DecryptedMessage> for EncryptedMessageStore {
    fn fetch(&self) -> Result<Vec<DecryptedMessage>, StorageError> {
        let conn = &mut self.conn()?;
        use self::schema::messages::dsl::*;

        messages
            .load::<DecryptedMessage>(conn)
            .map_err(StorageError::DieselResultError)
    }
}

impl Fetch<StoredSession> for EncryptedMessageStore {
    fn fetch(&self) -> Result<Vec<StoredSession>, StorageError> {
        let conn = &mut self.conn()?;
        use self::schema::sessions::dsl::*;

        sessions
            .load::<StoredSession>(conn)
            .map_err(StorageError::DieselResultError)
    }
}

impl Store<EncryptedMessageStore> for Account {
    fn store(&self, into: &EncryptedMessageStore) -> Result<(), StorageError> {
        let conn = &mut into.conn()?;

        diesel::insert_into(accounts::table)
            .values(NewStoredAccount::try_from(self)?)
            .execute(conn)
            .map_err(|e| StorageError::Store(e.to_string()))?;

        Ok(())
    }
}

impl Fetch<Account> for EncryptedMessageStore {
    fn fetch(&self) -> Result<Vec<Account>, StorageError> {
        use self::schema::accounts::dsl::*;
        let conn = &mut self.conn()?;

        let stored_accounts = accounts
            .order(created_at.desc())
            .load::<StoredAccount>(conn)
            .map_err(|e| StorageError::Store(e.to_string()))?;

        Ok(stored_accounts
            .iter()
            .map(|f| serde_json::from_slice(&f.serialized_key).unwrap())
            .collect())
    }
}

impl Store<EncryptedMessageStore> for StoredInstallation {
    fn store(&self, into: &EncryptedMessageStore) -> Result<(), StorageError> {
        let conn = &mut into.conn()?;
        diesel::insert_into(schema::installations::table)
            .values(self)
            .execute(conn)?;

        Ok(())
    }
}

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

#[cfg(test)]
mod tests {

    use super::{models::*, EncryptedMessageStore, StorageError, StorageOption};
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
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();

        NewDecryptedMessage::new(
            "Bola".into(),
            "0x000A".into(),
            "Hello Bola".into(),
            MessageState::Uninitialized as i32,
        )
        .store(&store)
        .unwrap();

        NewDecryptedMessage::new(
            "Mark".into(),
            "0x000A".into(),
            "Sup Mark".into(),
            MessageState::Uninitialized as i32,
        )
        .store(&store)
        .unwrap();

        NewDecryptedMessage::new(
            "Bola".into(),
            "0x000B".into(),
            "Hey Amal".into(),
            MessageState::Uninitialized as i32,
        )
        .store(&store)
        .unwrap();

        NewDecryptedMessage::new(
            "Bola".into(),
            "0x000A".into(),
            "bye".into(),
            MessageState::Uninitialized as i32,
        )
        .store(&store)
        .unwrap();

        let v: Vec<DecryptedMessage> = store.fetch().unwrap();
        assert_eq!(4, v.len());
    }

    #[test]
    fn store_persistent() {
        let db_path = format!("{}.db3", rand_string());
        {
            let store = EncryptedMessageStore::new(
                StorageOption::Persistent(db_path.clone()),
                EncryptedMessageStore::generate_enc_key(),
            )
            .unwrap();

            NewDecryptedMessage::new(
                "Bola".into(),
                "0x000A".into(),
                "Hello Bola".into(),
                MessageState::Uninitialized as i32,
            )
            .store(&store)
            .unwrap();

            let v: Vec<DecryptedMessage> = store.fetch().unwrap();
            assert_eq!(1, v.len());
        }

        fs::remove_file(db_path).unwrap();
    }

    #[test]
    fn message_roundtrip() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();

        let msg0 = NewDecryptedMessage::new(
            rand_string(),
            rand_string(),
            rand_vec(),
            MessageState::Uninitialized as i32,
        );
        sleep(Duration::from_millis(10));
        let msg1 = NewDecryptedMessage::new(
            rand_string(),
            rand_string(),
            rand_vec(),
            MessageState::Uninitialized as i32,
        );

        msg0.store(&store).unwrap();
        msg1.store(&store).unwrap();

        let msgs: Vec<DecryptedMessage> = store.fetch().unwrap();

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
            let store = EncryptedMessageStore::new(
                // StorageOption::Ephemeral,
                StorageOption::Persistent(db_path.clone()),
                enc_key.clone(),
            )
            .unwrap();

            let msg0 = NewDecryptedMessage::new(
                rand_string(),
                rand_string(),
                rand_vec(),
                MessageState::Uninitialized as i32,
            );
            msg0.store(&store).unwrap();
        } // Drop it

        enc_key[3] = 145; // Alter the enc_key
        let res = EncryptedMessageStore::new(StorageOption::Persistent(db_path.clone()), enc_key);
        // Ensure it fails
        match res.err() {
            Some(StorageError::DbInitError(_)) => (),
            _ => panic!("Expected a DbInitError"),
        }
        fs::remove_file(db_path).unwrap();
    }

    #[test]
    fn store_session() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();
        let session = StoredSession::new(
            rand_string(),
            rand_string(),
            rand_vec(),
            rand_string(), // user_address: rand_string(),
        );

        session.store(&store).unwrap();

        let results: Vec<StoredSession> = store.fetch().unwrap();
        assert_eq!(1, results.len());
    }
}
