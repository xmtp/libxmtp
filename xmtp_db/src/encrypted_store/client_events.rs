use super::DbConnection;
use crate::{StorageError, Store, impl_store, schema::client_events};
use diesel::{Identifiable, Insertable, Queryable, QueryableByName, prelude::*};
use serde::{Deserialize, Serialize};
use xmtp_common::time::now_ns;

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
#[diesel(primary_key(created_at_ns))]
pub struct ClientEvents {
    pub created_at_ns: i64,
    pub details: serde_json::Value,
}

impl_store!(ClientEvents, client_events);

impl ClientEvents {
    pub fn track(conn: &DbConnection, event: ClientEvent) -> Result<(), StorageError> {
        ClientEvents {
            created_at_ns: now_ns(),
            details: serde_json::to_value(&event).unwrap(),
        }
        .store(conn)
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
