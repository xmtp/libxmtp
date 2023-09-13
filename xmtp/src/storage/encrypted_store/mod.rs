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
    schema::{accounts, conversations, installations, messages, refresh_jobs, users, sessions},
};
use super::{now, StorageError};
use crate::{account::Account, utils::is_wallet_address, Errorer, Fetch, Store};
use diesel::{
    connection::SimpleConnection,
    prelude::*,
    r2d2::{ConnectionManager, Pool, PooledConnection},
    sql_query,
    sql_types::Text, result::DatabaseErrorKind,
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
        let pool = match opts {
            StorageOption::Ephemeral => Pool::builder()
                .max_size(1)
                .build(ConnectionManager::<SqliteConnection>::new(":memory:"))
                .map_err(|e| StorageError::DbInitError(e.to_string()))?,
            StorageOption::Persistent(ref path) => Pool::builder()
                .max_size(10)
                .build(ConnectionManager::<SqliteConnection>::new(path))
                .map_err(|e| StorageError::DbInitError(e.to_string()))?,
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
            .map_err(|e| StorageError::DbInitError(e.to_string()))?;

        Ok(())
    }

    pub fn create_fake_msg(&self, content: &str, state: i32) {
        NewStoredMessage::new("convo".into(), "addr".into(), content.into(), state, 10)
            .store(&mut self.conn().unwrap())
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
        let mut account_list: Vec<Account> = self.conn().unwrap().fetch_all()?;

        warn_length(&account_list, "StoredAccount", 1);

        Ok(account_list.pop())
    }

    pub fn get_latest_session_for_installation(
        &self,
        installation_id: &str,
        conn: &mut DbConnection,
    ) -> Result<Option<StoredSession>, StorageError> {
        use self::schema::sessions::dsl::*;

        sessions
            .filter(peer_installation_id.eq(installation_id))
            .order(updated_at.desc())
            .first(conn)
            .optional()
            .map_err(|e| StorageError::Unknown(e.to_string()))
    }

    pub fn get_latest_sessions_for_installation(
        &self,
        installation_id: &str,
        conn: &mut DbConnection,
    ) -> Result<Vec<StoredSession>, StorageError> {
        use self::schema::sessions::dsl::*;

        sessions
            .filter(peer_installation_id.eq(installation_id))
            .order(updated_at.desc())
            .get_results(conn)
            .map_err(|e| StorageError::Unknown(e.to_string()))
    }

    pub fn get_latest_sessions(
        &self,
        user_address: &str,
        conn: &mut DbConnection,
    ) -> Result<Vec<StoredSession>, StorageError> {
        if !is_wallet_address(user_address) {
            return Err(StorageError::Unknown(
                "incorrectly formatted walletAddress".into(),
            ));
        }
        let session_list = sql_query(
            "SELECT 
        sessions.* 
      FROM 
        (
          SELECT 
            DISTINCT First_value(session_id) OVER (
              partition BY peer_installation_id 
              ORDER BY 
                updated_at DESC
            ) AS session_id 
          FROM 
            sessions 
          WHERE 
            user_address = ?
        ) AS ids 
        LEFT JOIN sessions ON ids.session_id = sessions.session_id
      ",
        )
        .bind::<Text, _>(user_address)
        .load::<StoredSession>(conn)
        .map_err(|e| StorageError::Unknown(e.to_string()))?;

        Ok(session_list)
    }

    pub fn session_exists_for_installation(
        &self,
        installation_id: &str,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<bool, StorageError> {
        use self::schema::sessions::dsl as schema;

        let session_count: i64 = schema::sessions
            .filter(schema::peer_installation_id.eq(installation_id))
            .count()
            .get_result(conn)
            .map_err(|e| StorageError::Unknown(e.to_string()))?;

        Ok(session_count > 0)
    }

    pub fn get_installations(
        &self,
        conn: &mut DbConnection,
        user_address_str: &str,
    ) -> Result<Vec<StoredInstallation>, StorageError> {
        let installation_list = installations::table
            .filter(installations::user_address.eq(user_address_str))
            .load::<StoredInstallation>(conn)?;
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

    pub fn update_user_refresh_timestamp(
        &self,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
        user_address: &str,
        timestamp: i64,
    ) -> Result<StoredUser, StorageError> {
        diesel::update(users::table.find(user_address))
            .set(users::last_refreshed.eq(timestamp))
            .get_result::<StoredUser>(conn)
            .map_err(|e| e.into())
    }

    pub fn insert_user(&self, user: StoredUser) -> Result<(), StorageError> {
        let conn = &mut self.conn()?;
        self.insert_user_with_conn(conn, user)
    }

    pub fn insert_user_with_conn(
        &self,
        conn: &mut DbConnection,
        user: StoredUser,
    ) -> Result<(), StorageError> {
        match diesel::insert_into(users::table)
        .values(user)
        .execute(conn)
        {
            Ok(_) => Ok(()),
            Err(Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {
                Ok(())
            }
            Err(error) => Err(StorageError::from(error)),
        }?;
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

    pub fn insert_conversation(
        &self,
        conversation: StoredConversation,
    ) -> Result<(), StorageError> {
        let conn = &mut self.conn()?;
        self.insert_conversation_with_conn(conn, conversation)
    }

    pub fn insert_conversation_with_conn(
        &self,
        conn: &mut DbConnection,
        conversation: StoredConversation,
    ) -> Result<(), StorageError> {
        match diesel::insert_into(conversations::table)
        .values(conversation)
        .execute(conn)
        {
            Ok(_) => Ok(()),
            Err(Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {
                Ok(())
            }
            Err(error) => Err(StorageError::from(error)),
        }?;
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

    pub fn get_unprocessed_messages(&self) -> Result<Vec<StoredMessage>, StorageError> {
        let conn = &mut self.conn()?;

        let msg_list = messages::table
            .filter(messages::state.eq(MessageState::Unprocessed as i32))
            .load::<StoredMessage>(conn)?;

        Ok(msg_list)
    }

    pub fn lock_refresh_job<F>(&self, kind: RefreshJobKind, cb: F) -> Result<(), StorageError>
    where
        F: FnOnce(
            &mut PooledConnection<ConnectionManager<SqliteConnection>>,
            RefreshJob,
        ) -> Result<(), StorageError>,
    {
        let conn = &mut self.conn()?;
        conn.transaction::<(), StorageError, _>(|connection| {
            let start_time = now();
            let job: RefreshJob = refresh_jobs::table
                .find::<String>(kind.to_string())
                .first::<RefreshJob>(connection)?;

            let result = cb(connection, job);

            if result.is_ok() {
                diesel::update(refresh_jobs::table.find(kind.to_string()))
                    .set(refresh_jobs::last_run.eq(start_time))
                    .get_result::<RefreshJob>(connection)?;
            } else {
                return result;
            }

            Ok(())
        })?;

        Ok(())
    }

    pub fn get_inbound_messages(
        &self,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
        status: InboundMessageStatus,
    ) -> Result<Vec<InboundMessage>, StorageError> {
        use self::schema::inbound_messages::dsl as schema;

        let msgs = schema::inbound_messages
            .filter(schema::status.eq(status as i16))
            .order(schema::sent_at_ns.asc())
            .load::<InboundMessage>(conn)?;

        Ok(msgs)
    }
    pub fn save_inbound_message(
        &self,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
        message: InboundMessage,
    ) -> Result<(), StorageError> {
        use self::schema::inbound_messages::dsl as schema;
        let mesg_id = message.id.clone();
        let result = diesel::insert_into(schema::inbound_messages)
            .values(message)
            .execute(conn);

        if let Err(e) = result {
            use diesel::result as dr;
            match &e {
                dr::Error::DatabaseError(dr::DatabaseErrorKind::UniqueViolation, _) => {
                    warn!("This message has already been stored: {}", mesg_id)
                }
                _ => return Err(StorageError::from(e)),
            }
        }

        Ok(())
    }

    pub fn set_msg_status(
        &self,
        conn: &mut DbConnection,
        id: String,
        status: InboundMessageStatus,
    ) -> Result<(), StorageError> {
        use self::schema::inbound_messages::dsl as schema;

        diesel::update(schema::inbound_messages)
            .filter(schema::id.eq(id))
            .set(schema::status.eq(status as i16))
            .execute(conn)?;

        Ok(())
    }

    pub fn insert_install(
        &self,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
        install: StoredInstallation,
    ) -> Result<(), StorageError> {
        match diesel::insert_into(installations::table)
        .values(install)
        .execute(conn)
        {
            Ok(_) => Ok(()),
            Err(Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {
                Ok(())
            }
            Err(error) => Err(StorageError::from(error)),
        }?;
        Ok(())    
    }

    pub fn insert_session(
        &self,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
        session: StoredSession,
    ) -> Result<(), StorageError> {
        match diesel::insert_into(sessions::table)
        .values(session)
        .execute(conn)
        {
            Ok(_) => Ok(()),
            Err(Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {
                Ok(())
            }
            Err(error) => Err(StorageError::from(error)),
        }?;
        Ok(())
   }

    pub fn insert_message(
        &self,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
        msg: NewStoredMessage,
    ) -> Result<(), StorageError> {
        match diesel::insert_into(messages::table)
        .values(msg)
        .execute(conn)
        {
            Ok(_) => Ok(()),
            Err(Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {
                Ok(())
            }
            Err(error) => Err(StorageError::from(error)),
        }?;
        Ok(())    
    }

    pub fn commit_outbound_payloads_for_message(
        &self,
        message_id: i32,
        updated_message_state: MessageState,
        new_outbound_payloads: Vec<StoredOutboundPayload>,
        updated_sessions: Vec<StoredSession>,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<(), StorageError> {
        for session in updated_sessions {
            diesel::update(schema::sessions::table.find(session.session_id))
                .set(schema::sessions::vmac_session_data.eq(session.vmac_session_data))
                .get_result::<StoredSession>(conn)?;
        }
        diesel::insert_into(schema::outbound_payloads::table)
            .values(new_outbound_payloads)
            .execute(conn)?;
        diesel::update(messages::table.find(message_id))
            .set(messages::state.eq(updated_message_state as i32))
            .get_result::<StoredMessage>(conn)?;
        Ok(())
    }

    pub fn fetch_and_lock_outbound_payloads(
        &self,
        payload_state: OutboundPayloadState,
        lock_duration_ns: i64,
    ) -> Result<Vec<StoredOutboundPayload>, StorageError> {
        let conn = &mut self.conn()?;
        use self::schema::outbound_payloads::dsl as schema;
        let now = now();
        // Must happen atomically
        let payloads = diesel::update(schema::outbound_payloads)
            .filter(schema::outbound_payload_state.eq(payload_state as i32))
            .filter(schema::locked_until_ns.lt(now))
            .set(schema::locked_until_ns.eq(now + lock_duration_ns))
            .get_results::<StoredOutboundPayload>(conn)?;
        Ok(payloads)
    }

    pub fn update_and_unlock_outbound_payloads(
        &self,
        payload_ids: Vec<i64>,
        new_payload_state: OutboundPayloadState,
    ) -> Result<(), StorageError> {
        let conn = &mut self.conn()?;
        use self::schema::outbound_payloads::dsl::*;
        diesel::update(outbound_payloads)
            .filter(created_at_ns.eq_any(payload_ids))
            .set((
                outbound_payload_state.eq(new_payload_state as i32),
                locked_until_ns.eq(0),
            ))
            .execute(conn)?;
        Ok(())
    }

    pub fn get_conversations(
        &self,
        conn: &mut DbConnection,
    ) -> Result<Vec<StoredConversation>, StorageError> {
        let convos = conversations::table.load::<StoredConversation>(conn)?;
        Ok(convos)
    }

    pub fn get_stored_messages(
        &self,
        conn: &mut DbConnection,
        allowed_states: Option<Vec<MessageState>>,
        conversation_id: Option<&str>,
        start_time_ns: Option<i64>,
        end_time_ns: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<StoredMessage>, StorageError> {
        use self::schema::messages::dsl as schema;

        let mut query = schema::messages
            .order(schema::sent_at_ns.asc())
            .into_boxed();

        if let Some(allowed_states) = allowed_states {
            query =
                query.filter(schema::state.eq_any(allowed_states.into_iter().map(|s| s as i32)));
        }

        if let Some(conversation_id) = conversation_id {
            query = query.filter(schema::convo_id.eq(conversation_id));
        }

        if let Some(start_time_ns) = start_time_ns {
            query = query.filter(schema::sent_at_ns.ge(start_time_ns));
        }

        if let Some(end_time_ns) = end_time_ns {
            query = query.filter(schema::sent_at_ns.le(end_time_ns));
        }

        if let Some(limit) = limit {
            query = query.limit(limit);
        }

        let msgs = query.load::<StoredMessage>(conn)?;

        Ok(msgs)
    }
}

impl Store<DbConnection> for NewStoredMessage {
    fn store(&self, into: &mut DbConnection) -> Result<(), StorageError> {
        diesel::insert_into(messages::table)
            .values(self)
            .execute(into)?;

        Ok(())
    }
}

impl Store<DbConnection> for StoredUser {
    fn store(&self, into: &mut DbConnection) -> Result<(), StorageError> {
        diesel::insert_into(users::table)
            .values(self)
            .execute(into)?;

        Ok(())
    }
}

impl Store<DbConnection> for StoredConversation {
    fn store(&self, into: &mut DbConnection) -> Result<(), StorageError> {
        diesel::insert_into(conversations::table)
            .values(self)
            .execute(into)?;

        Ok(())
    }
}

impl Store<DbConnection> for StoredSession {
    fn store(&self, into: &mut DbConnection) -> Result<(), StorageError> {
        diesel::insert_into(schema::sessions::table)
            .values(self)
            .execute(into)?;

        Ok(())
    }
}

impl Fetch<StoredMessage> for DbConnection {
    type Key<'a> = i32;
    fn fetch_all(&mut self) -> Result<Vec<StoredMessage>, StorageError> {
        use self::schema::messages::dsl::*;

        messages
            .load::<StoredMessage>(self)
            .map_err(StorageError::DieselResultError)
    }

    fn fetch_one(&mut self, key: i32) -> Result<Option<StoredMessage>, StorageError> where {
        use self::schema::messages::dsl::*;
        Ok(messages.find(key).first(self).optional()?)
    }
}

impl Fetch<StoredSession> for DbConnection {
    type Key<'a> = &'a str;
    fn fetch_all(&mut self) -> Result<Vec<StoredSession>, StorageError> {
        use self::schema::sessions::dsl::*;

        sessions
            .load::<StoredSession>(self)
            .map_err(StorageError::DieselResultError)
    }

    fn fetch_one(&mut self, key: &str) -> Result<Option<StoredSession>, StorageError> {
        use self::schema::sessions::dsl::*;
        Ok(sessions.find(key).first(self).optional()?)
    }
}

impl Fetch<StoredUser> for DbConnection {
    type Key<'a> = &'a str;
    fn fetch_all(&mut self) -> Result<Vec<StoredUser>, StorageError> {
        use self::schema::users::dsl;

        dsl::users
            .load::<StoredUser>(self)
            .map_err(StorageError::DieselResultError)
    }
    fn fetch_one(&mut self, key: &str) -> Result<Option<StoredUser>, StorageError> {
        use self::schema::users::dsl::*;
        Ok(users.find(key).first(self).optional()?)
    }
}

impl Fetch<StoredConversation> for DbConnection {
    type Key<'a> = &'a str;
    fn fetch_all(&mut self) -> Result<Vec<StoredConversation>, StorageError> {
        use self::schema::conversations::dsl;

        dsl::conversations
            .load::<StoredConversation>(self)
            .map_err(StorageError::DieselResultError)
    }
    fn fetch_one(&mut self, key: &str) -> Result<Option<StoredConversation>, StorageError> {
        use self::schema::conversations::dsl::*;
        Ok(conversations.find(key).first(self).optional()?)
    }
}

impl Store<DbConnection> for Account {
    fn store(&self, into: &mut DbConnection) -> Result<(), StorageError> {
        diesel::insert_into(accounts::table)
            .values(NewStoredAccount::try_from(self)?)
            .execute(into)
            .map_err(|e| StorageError::Store(e.to_string()))?;

        Ok(())
    }
}

impl Fetch<Account> for DbConnection {
    type Key<'a> = i32;
    fn fetch_all(&mut self) -> Result<Vec<Account>, StorageError> {
        use self::schema::accounts::dsl::*;

        let stored_accounts = accounts
            .order(created_at.desc())
            .load::<StoredAccount>(self)
            .map_err(|e| StorageError::Store(e.to_string()))?;

        Ok(stored_accounts
            .iter()
            .map(|f| serde_json::from_slice(&f.serialized_key).unwrap())
            .collect())
    }

    fn fetch_one(&mut self, key: i32) -> Result<Option<Account>, StorageError> {
        use self::schema::accounts::dsl::*;
        let stored_account: Option<StoredAccount> = accounts.find(key).first(self).optional()?;

        match stored_account {
            None => Ok(None),
            Some(a) => serde_json::from_slice(&a.serialized_key)
                .map_err(|e| StorageError::Unknown(format!("Failed to deserialize key:{}", e))),
        }
    }
}

impl Store<DbConnection> for StoredInstallation {
    fn store(&self, into: &mut DbConnection) -> Result<(), StorageError> {
        diesel::insert_into(schema::installations::table)
            .values(self)
            .execute(into)?;

        Ok(())
    }
}

impl Fetch<StoredInstallation> for DbConnection {
    type Key<'a> = &'a str;
    fn fetch_all(&mut self) -> Result<Vec<StoredInstallation>, StorageError> {
        use self::schema::installations::dsl;

        dsl::installations
            .load::<StoredInstallation>(self)
            .map_err(StorageError::DieselResultError)
    }
    fn fetch_one(&mut self, key: &str) -> Result<Option<StoredInstallation>, StorageError> {
        use self::schema::installations::dsl::*;
        Ok(installations.find(key).first(self).optional()?)
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
        let conn = &mut store.conn().unwrap();
        NewStoredMessage::new(
            "Bola".into(),
            "0x000A".into(),
            "Hello Bola".into(),
            MessageState::Unprocessed as i32,
            10,
        )
        .store(conn)
        .unwrap();

        NewStoredMessage::new(
            "Mark".into(),
            "0x000A".into(),
            "Sup Mark".into(),
            MessageState::Unprocessed as i32,
            10,
        )
        .store(conn)
        .unwrap();

        NewStoredMessage::new(
            "Bola".into(),
            "0x000B".into(),
            "Hey Amal".into(),
            MessageState::Unprocessed as i32,
            10,
        )
        .store(conn)
        .unwrap();

        NewStoredMessage::new(
            "Bola".into(),
            "0x000A".into(),
            "bye".into(),
            MessageState::Unprocessed as i32,
            10,
        )
        .store(conn)
        .unwrap();

        let v: Vec<StoredMessage> = conn.fetch_all().unwrap();
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
            let conn = &mut store.conn().unwrap();

            NewStoredMessage::new(
                "Bola".into(),
                "0x000A".into(),
                "Hello Bola".into(),
                MessageState::Unprocessed as i32,
                10,
            )
            .store(conn)
            .unwrap();

            let v: Vec<StoredMessage> = conn.fetch_all().unwrap();
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

        let msg0 = NewStoredMessage::new(
            rand_string(),
            rand_string(),
            rand_vec(),
            MessageState::Unprocessed as i32,
            10,
        );
        sleep(Duration::from_millis(10));
        let msg1 = NewStoredMessage::new(
            rand_string(),
            rand_string(),
            rand_vec(),
            MessageState::Unprocessed as i32,
            10,
        );

        let conn = &mut store.conn().unwrap();

        msg0.store(conn).unwrap();
        msg1.store(conn).unwrap();

        let msgs: Vec<StoredMessage> = conn.fetch_all().unwrap();

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

            let msg0 = NewStoredMessage::new(
                rand_string(),
                rand_string(),
                rand_vec(),
                MessageState::Unprocessed as i32,
                10,
            );
            msg0.store(&mut store.conn().unwrap()).unwrap();
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
        let conn = &mut store.conn().unwrap();

        let install_id = rand_string();

        let session_a = StoredSession::new(
            "A".into(),
            install_id.clone(),
            rand_vec(),
            rand_string(), // user_address: rand_string(),
        );
        session_a.store(conn).unwrap();

        let results: Vec<StoredSession> = conn.fetch_all().unwrap();
        assert_eq!(1, results.len());

        let session_b = StoredSession::new(
            "B".into(),
            install_id.clone(),
            rand_vec(),
            rand_string(), // user_address: rand_string(),
        );
        session_b.store(conn).unwrap();

        let results: Vec<StoredSession> = conn.fetch_all().unwrap();
        assert_eq!(2, results.len());

        let latest_session = store
            .get_latest_session_for_installation(&install_id, conn)
            .unwrap()
            .expect("No session found");

        assert_eq!(latest_session.session_id, session_b.session_id);
    }

    #[test]
    fn lock_refresh_job() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();

        store
            .lock_refresh_job(RefreshJobKind::Message, |_, job| {
                assert_eq!(job.id, RefreshJobKind::Message.to_string());
                assert_eq!(job.last_run, 0);

                Ok(())
            })
            .unwrap();

        store
            .lock_refresh_job(RefreshJobKind::Message, |_, job| {
                assert!(job.last_run > 0);

                Ok(())
            })
            .unwrap();
        
        let mut last_run = 0;
        let res_expected_err = store.lock_refresh_job(RefreshJobKind::Message, |_, job| {
            assert_eq!(job.id, RefreshJobKind::Message.to_string());
            last_run = job.last_run;

            Err(StorageError::Unknown(String::from("RefreshJob failed")))
        });
        assert!(res_expected_err.is_err());

        store
            .lock_refresh_job(RefreshJobKind::Message, |_, job| {
                // Ensure that last run time does not change if the job fails
                assert_eq!(job.last_run, last_run);

                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn get_conversations() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();

        let address = String::from("0x01");
        let conn = &mut store.conn().unwrap();

        let convo_1 = StoredConversation {
            convo_id: "convo_1".into(),
            peer_address: address.clone(),
            created_at: 10,
        };
        let convo_2 = StoredConversation {
            convo_id: "convo_2".into(),
            peer_address: address.clone(),
            created_at: 10,
        };
        let user_1 = StoredUser {
            user_address: address.clone(),
            created_at: 10,
            last_refreshed: 0,
        };

        user_1.store(conn).unwrap();
        convo_1.store(conn).unwrap();
        convo_2.store(conn).unwrap();

        let conversations = store.get_conversations(conn).unwrap();
        assert_eq!(2, conversations.len());
    }

    #[test]
    fn errors_when_no_update() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();

        let conn = &mut store.conn().unwrap();
        let result = store.update_user_refresh_timestamp(conn, "0x01", 1);
        assert!(result.is_err());
    }

    #[test]
    fn get_stored_messages() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();

        let conn = &mut store.conn().unwrap();
        let convo_id = "convo_1";

        NewStoredMessage::new(
            convo_id.to_string(),
            "0x000A".into(),
            "Hello Bola".into(),
            MessageState::LocallyCommitted as i32,
            10,
        )
        .store(conn)
        .unwrap();

        NewStoredMessage::new(
            convo_id.to_string(),
            "0x000A".into(),
            "Hello again".into(),
            MessageState::Received as i32,
            20,
        )
        .store(conn)
        .unwrap();

        NewStoredMessage::new(
            "convo_2".into(),
            "0x000A".into(),
            "Hello from convo 2".into(),
            MessageState::Received as i32,
            30,
        )
        .store(conn)
        .unwrap();

        let convo_1_results = store
            .get_stored_messages(conn, None, Some(convo_id), None, None, None)
            .unwrap();
        assert_eq!(2, convo_1_results.len());
        // Ensure results are properly sorted
        assert!(convo_1_results[0].sent_at_ns < convo_1_results[1].sent_at_ns);

        let results_with_received_state = store
            .get_stored_messages(
                conn,
                Some(vec![MessageState::Received]),
                None,
                None,
                None,
                None,
            )
            .unwrap();
        assert_eq!(2, results_with_received_state.len());
        assert_eq!("convo_2", results_with_received_state[1].convo_id);

        let results_with_time_filter =
            store.get_stored_messages(conn, None, None, Some(11), Some(20), None);
        assert_eq!(1, results_with_time_filter.unwrap().len());

        let results_with_limit = store
            .get_stored_messages(conn, None, None, None, None, Some(1))
            .unwrap();
        assert_eq!(1, results_with_limit.len());
    }
}
