pub mod models;
pub mod schema;

use std::time::{SystemTime, UNIX_EPOCH};

use self::{models::*, schema::messages};
use crate::types::Message;
use diesel::{prelude::*, sql_query, Connection};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
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

pub trait MsgStore {
    // Client Required Methods
    fn insert_message(account_id: String, msg: Message) -> Result<(), StoreError>;

    // App Required Methods
    // fn list_conversations() -> Vec<&Conversation>;
    // fn get_conversation(cursor: u64) -> Vec<&Message>;
    // fn delete_conversation(convo_id: String) -> Result<(),StoreError>;

    // fn get_message()-> &Message;
    // fn delete_message() ->  Result<(),StoreError>;
    // fn on_new_message() -> ??;
}

enum StorageOption {
    Ephemeral,
    Peristent(String),
}

impl Default for StorageOption {
    fn default() -> Self {
        StorageOption::Ephemeral
    }
}

struct UnencryptedMessageStore {
    connect_opt: StorageOption,
    conn: SqliteConnection,
}

impl UnencryptedMessageStore {
    fn new(opts: StorageOption) -> Result<Self, StoreError> {
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

    fn get_messages(&mut self, convo_id: &str) -> Result<(), StoreError> {
        use self::schema::messages::dsl::*;
        let conn = &mut self.conn;
        let results = messages
            .filter(convoid.eq(convo_id))
            .limit(5)
            .load::<DecryptedMessage>(conn)
            .expect("Error loading messages");

        println!("Displaying {} messages", results.len());
        for msg in results {
            println!(
                "[{}]   {}   ->>  {}",
                msg.created_at, msg.addr_from, msg.content
            );
        }
        Ok(())
    }

    // TODO: Change signature to support encoded content
    pub fn insert_message(
        &mut self,
        convo_id: String,
        addr_from: String,
        content: String,
    ) -> Result<(), StoreError> {
        let conn = &mut self.conn;

        let new_msg = NewDecryptedMessage {
            created_at: now(),
            convoid: convo_id,
            addr_from,
            content,
        };

        diesel::insert_into(messages::table)
            .values(&new_msg)
            .execute(conn)
            .expect("Error saving new message");

        Ok(())
    }
}

// Diesel + Sqlite is giving trouble when trying to use f64 with REAL type. Downgraded to f32 timestamps
fn now() -> f32 {
    let start = SystemTime::now();
    start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs_f32()
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

    use super::{DbInitializer, StorageOption, StoreError, UnencryptedMessageStore, DB_PATH};

    #[test]
    fn store_init() {
        let mut store = UnencryptedMessageStore::new(StorageOption::Ephemeral).unwrap();
        store
            .insert_message("Bola".into(), "0x000A".into(), "Hello Bola".into())
            .unwrap();
        store
            .insert_message("Mark".into(), "0x000A".into(), "Hi Mark".into())
            .unwrap();
        store
            .insert_message("Bola".into(), "0x000B".into(), "Hi Amal".into())
            .unwrap();
        store.get_messages("Bola".into()).unwrap();
    }

    #[test]
    fn task_init_db() {
        let conn = &mut SqliteConnection::establish(DB_PATH)
            .map_err(StoreError::DieselConnectError)
            .unwrap();
        DbInitializer::initialize(conn).unwrap()
    }
}
