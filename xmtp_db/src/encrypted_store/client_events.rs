use std::sync::atomic::{AtomicBool, Ordering};

use super::{
    ConnectionExt, DbConnection, group::ConversationType, group_intent::IntentKind,
    schema::client_events::dsl,
};
use crate::{Store, impl_store, schema::client_events};
use diesel::{Insertable, Queryable, associations::HasTable, prelude::*};
use serde::{Deserialize, Serialize};
use xmtp_common::{NS_IN_30_DAYS, time::now_ns};

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = client_events)]
#[diesel(primary_key(created_at_ns))]
pub struct ClientEvents {
    pub created_at_ns: i64,
    pub group_id: Option<Vec<u8>>,
    pub details: serde_json::Value,
}

impl_store!(ClientEvents, client_events);

pub static EVENTS_ENABLED: AtomicBool = AtomicBool::new(true);

impl ClientEvents {
    pub fn track<C: ConnectionExt>(
        db: &DbConnection<C>,
        group_id: Option<Vec<u8>>,
        event: impl AsRef<ClientEvent>,
    ) {
        if !EVENTS_ENABLED.load(Ordering::Relaxed) {
            return;
        }

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
            group_id,
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

    fn clear_old_events<C: ConnectionExt>(
        db: &DbConnection<C>,
    ) -> Result<(), crate::ConnectionError> {
        db.raw_query_write(|db| {
            diesel::delete(
                dsl::client_events.filter(dsl::created_at_ns.lt(now_ns() - NS_IN_30_DAYS)),
            )
            .execute(db)?;
            Ok(())
        })
    }

    pub fn all_events(db: &DbConnection) -> Result<Vec<Self>, crate::ConnectionError> {
        Ok(db.raw_query_read(|db| dsl::client_events.load(db))?)
    }

    pub fn all_events_paged<C: ConnectionExt>(
        db: &C,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, crate::ConnectionError> {
        let query = dsl::client_events::table()
            .order_by(dsl::created_at_ns.asc())
            .limit(limit)
            .offset(offset);
        db.raw_query_read(|db| query.load(db))
    }

    pub fn key_updates(db: &DbConnection) -> Result<Vec<Self>, crate::ConnectionError> {
        Ok(db.raw_query_read(|db| {
            let query = dsl::client_events.filter(diesel::dsl::sql::<diesel::sql_types::Bool>(
                "jsonb_extract(details, '$.QueueIntent.intent_kind') = 'KeyUpdate'",
            ));

            query.load::<ClientEvents>(db)
        })?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientEvent {
    ClientBuild,
    Generic(String),
    QueueIntent(EvtQueueIntent),
    EpochChange(EvtEpochChange),
    GroupWelcome(EvtGroupWelcome),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EvtQueueIntent {
    pub intent_kind: IntentKind,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct EvtGroupWelcome {
    pub conversation_type: ConversationType,
    pub added_by_inbox_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EvtEpochChange {
    pub prev_epoch: i64,
    pub new_epoch: i64,
    pub cursor: i64,
}

impl AsRef<ClientEvent> for ClientEvent {
    fn as_ref(&self) -> &ClientEvent {
        self
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        Store,
        client_events::{ClientEvent, ClientEvents, EvtQueueIntent},
        group_intent::IntentKind,
        with_connection,
    };

    #[xmtp_common::test(unwrap_try = "true")]
    // A client build event should clear old events.
    async fn clear_old_events() {
        with_connection(|conn| {
            ClientEvents {
                created_at_ns: 0,
                group_id: None,
                details: serde_json::to_value(ClientEvent::ClientBuild)?,
            }
            .store(conn)?;
            ClientEvents {
                created_at_ns: 0,
                group_id: None,
                details: serde_json::to_value(ClientEvent::QueueIntent(EvtQueueIntent {
                    intent_kind: IntentKind::KeyUpdate,
                }))?,
            }
            .store(conn)?;

            let all = ClientEvents::all_events(conn)?;
            assert_eq!(all.len(), 2);

            ClientEvents::track(conn, None, ClientEvent::ClientBuild);
            let all = ClientEvents::all_events(conn)?;
            assert_eq!(all.len(), 1);
        })
        .await;
    }
}
