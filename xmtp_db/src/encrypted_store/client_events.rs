use super::DbConnection;
use crate::{StorageError, schema::client_events};
use diesel::{Identifiable, Insertable, Queryable, QueryableByName};
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Insertable,
    Identifiable,
    Queryable,
    Eq,
    PartialEq,
    QueryableByName,
)]
#[diesel(table_name = client_events)]
#[diesel(primary_key(id))]
pub struct ClientEvents {
    pub id: i32,
    pub created_at_ns: i64,
    pub event: i32,
    pub details: serde_json::Value,
}

impl ClientEvents {
    pub fn track(conn: &DbConnection, event: ClientEvent) -> Result<(), StorageError> {
        Ok(())
    }
}

#[repr(i32)]
#[derive(Serialize, Deserialize)]
pub enum ClientEvent {
    KeyChange(KeyChangeDetails) = 1,
}

#[derive(Serialize, Deserialize)]
pub struct KeyChangeDetails {
    pub group_id: Vec<u8>,
}
