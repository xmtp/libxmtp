use super::{
    ConnectionExt, DbConnection, group::ConversationType, group_intent::IntentKind,
    schema::client_events::dsl,
};
use crate::{StorageError, Store, impl_store, schema::client_events};
use diesel::{Insertable, Queryable, prelude::*};
use serde::{Deserialize, Serialize};
use xmtp_common::{NS_IN_30_DAYS, time::now_ns};

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = client_events)]
#[diesel(primary_key(created_at_ns))]
pub struct ClientEvents {
    pub created_at_ns: i64,
    pub details: serde_json::Value,
}

impl_store!(ClientEvents, client_events);

impl ClientEvents {
    pub fn track<C: ConnectionExt>(db: &DbConnection<C>, event: impl AsRef<ClientEvent>) {
        let event = event.as_ref();

        let details = match serde_json::to_value(event) {
            Ok(details) => details,
            Err(err) => {
                tracing::warn!("ClientEvents: unable to serialize event. {err:?}");
                return;
            }
        };

        let result = ClientEvents {
            created_at_ns: now_ns(),
            details,
        }
        .store(db);
        if let Err(err) = result {
            // We don't want ClientEvents causing any issues, so we just warn if something goes wrong.
            tracing::warn!("ClientEvents: {err:?}");
        }

        // Clear old events on build.
        if matches!(event, ClientEvent::ClientBuild) {
            if let Err(err) = Self::clear_old_events(db) {
                tracing::warn!("ClientEvents clear old events: {err:?}");
            }
        }
    }

    fn clear_old_events<C: ConnectionExt>(db: &DbConnection<C>) -> Result<(), StorageError> {
        Ok(db.raw_query_write(|db| {
            diesel::delete(
                dsl::client_events.filter(dsl::created_at_ns.lt(now_ns() - NS_IN_30_DAYS)),
            )
            .execute(db)?;
            Ok(())
        })?)
    }

    pub fn all_events(db: &DbConnection) -> Result<Vec<Self>, StorageError> {
        Ok(db.raw_query_read(|db| dsl::client_events.load(db))?)
    }

    pub fn key_updates(db: &DbConnection) -> Result<Vec<Self>, StorageError> {
        Ok(db.raw_query_read(|db| {
            let query = dsl::client_events.filter(diesel::dsl::sql::<diesel::sql_types::Bool>(
                "json_extract(details, '$.QueueIntent.intent_kind') = 'KeyUpdate'",
            ));

            query.load::<ClientEvents>(db)
        })?)
    }
}

#[derive(Serialize, Deserialize)]
pub enum ClientEvent {
    ClientBuild,
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
