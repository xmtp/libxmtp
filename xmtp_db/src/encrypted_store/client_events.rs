use super::{
    ConnectionExt, DbConnection, group::ConversationType, group_intent::IntentKind,
    schema::client_events::dsl,
};
use crate::{StorageError, Store, impl_store, schema::client_events};
use diesel::{Insertable, Queryable, prelude::*};
use serde::{Deserialize, Serialize};
use xmtp_common::time::now_ns;

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = client_events)]
#[diesel(primary_key(created_at_ns))]
pub struct ClientEvents {
    pub created_at_ns: i64,
    pub details: serde_json::Value,
}

impl_store!(ClientEvents, client_events);

impl ClientEvents {
    pub fn track<C: ConnectionExt>(
        conn: &DbConnection<C>,
        event: impl AsRef<ClientEvent>,
    ) -> Result<(), StorageError> {
        ClientEvents {
            created_at_ns: now_ns(),
            details: serde_json::to_value(event.as_ref()).unwrap(),
        }
        .store(conn)
    }

    pub fn all_events(conn: &DbConnection) -> Result<Vec<Self>, StorageError> {
        Ok(conn.raw_query_read(|conn| dsl::client_events.load(conn))?)
    }

    pub fn key_updates(conn: &DbConnection) -> Result<Vec<Self>, StorageError> {
        Ok(conn.raw_query_read(|conn| {
            let query = dsl::client_events.filter(diesel::dsl::sql::<diesel::sql_types::Bool>(
                "json_extract(details, '$.QueueIntent.intent_kind') = 'KeyUpdate'",
            ));

            query.load::<ClientEvents>(conn)
        })?)
    }
}

#[derive(Serialize, Deserialize)]
pub enum ClientEvent {
    QueueIntent {
        group_id: Vec<u8>,
        intent_kind: IntentKind,
    },
    WelcomedIntoGroup {
        group_id: Vec<u8>,
        conversation_type: ConversationType,
        added_by_inbox_id: String,
    },
}

impl AsRef<ClientEvent> for ClientEvent {
    fn as_ref(&self) -> &ClientEvent {
        self
    }
}
