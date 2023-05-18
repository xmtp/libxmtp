pub mod models;
pub mod schema;
use self::{models::*, schema::messages};
use crate::{Fetch, Store};
use diesel::{prelude::*, sql_query, Connection};
use thiserror::Error;

const DB_PATH: &str = "./xmtp_embedded.db3";

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("SqliteError raised")]
    SqliteError(#[from] rusqlite::Error),
    #[error("Diesel connection error")]
    DieselConnectError(#[from] diesel::ConnectionError),
    #[error("Diesel result error")]
    DieselResultError(#[from] diesel::result::Error),
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

impl Default for UnencryptedMessageStore {
    fn default() -> Self {
        Self::new(StorageOption::Ephemeral)
            .expect("Error Occured: tring to create default Ephemeral store")
    }
}

impl UnencryptedMessageStore {
    pub fn new(opts: StorageOption) -> Result<Self, StoreError> {
        let db_path = match opts {
            StorageOption::Ephemeral => ":memory:",
            StorageOption::Peristent(ref path) => path,
        };

        let conn = SqliteConnection::establish(db_path).map_err(StoreError::DieselConnectError)?;
        let mut obj = Self {
            connect_opt: opts,
            conn,
        };

        obj.init_db()?;
        Ok(obj)
    }

    fn init_db(&mut self) -> Result<(), StoreError> {
        DbInitializer::initialize(&mut self.conn)
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
    type E = StoreError;
    fn fetch(&mut self) -> Result<Vec<DecryptedMessage>, Self::E> {
        use self::schema::messages::dsl::*;
        messages
            .limit(5)
            .load::<DecryptedMessage>(&mut self.conn)
            .map_err(StoreError::DieselResultError)
    }
}

// TODO: Repalce with Embedded Migrations
pub(crate) struct DbInitializer {}

impl DbInitializer {
    pub fn initialize(conn: &mut SqliteConnection) -> Result<(), StoreError> {
        Self::create_table(conn, Self::table_stmt_messages())?;
        Self::create_table(conn, Self::table_stmt_channels())?;

        Ok(())
    }

    fn create_table(conn: &mut SqliteConnection, stmt: &str) -> Result<(), StoreError> {
        sql_query(stmt)
            .execute(conn)
            .map_err(StoreError::DieselResultError)?;

        Ok(())
    }

    fn table_stmt_messages() -> &'static str {
        "CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY NOT NULL,
            created_at REAL NOT NULL,
            convoid TEXT NOT NULL,
            addr_from TEXT NOT NULL,
            content TEXT NOT NULL
          )"
    }

    fn table_stmt_channels() -> &'static str {
        "CREATE TABLE IF NOT EXISTS channels (
            id INTEGER PRIMARY KEY NOT NULL,
            channel_type TEXT NOT NULL,
            created_at REAL NOT NULL,
            display_name TEXT NOT NULL,
            members TEXT NOT NULL
          )"
    }
}

#[cfg(test)]
mod tests {
    use diesel::{Connection, SqliteConnection};

    use crate::{Fetch, Store};

    use super::{
        models::{DecryptedMessage, NewDecryptedMessage},
        DbInitializer, StorageOption, StoreError, UnencryptedMessageStore, DB_PATH,
    };

    #[allow(dead_code)]
    fn task_init_db() {
        let conn = &mut SqliteConnection::establish(DB_PATH)
            .map_err(StoreError::DieselConnectError)
            .unwrap();
        DbInitializer::initialize(conn).unwrap()
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

    // TODO: Add Content Tests
}
