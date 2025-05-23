use super::{
    ConnectionExt, DbConnection, consent_record::ConsentState, group::ConversationType,
    group_intent::IntentKind, schema::client_events::dsl,
};
use crate::{Store, impl_store, schema::client_events};
use diesel::{Insertable, Queryable, associations::HasTable, prelude::*};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use xmtp_common::{NS_IN_30_DAYS, time::now_ns};

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = client_events)]
#[diesel(primary_key(created_at_ns))]
pub struct ClientEvents {
    pub created_at_ns: i64,
    pub group_id: Option<Vec<u8>>,
    pub event: String,
    pub details: serde_json::Value,
}

impl_store!(ClientEvents, client_events);

pub static EVENTS_ENABLED: AtomicBool = AtomicBool::new(true);

impl ClientEvents {
    #[allow(invalid_type_param_default)]
    pub fn track<C: ConnectionExt>(
        db: &DbConnection<C>,
        group_id: Option<Vec<u8>>,
        event: impl AsRef<ClientEvent>,
        details: impl Serialize,
    ) {
        if !EVENTS_ENABLED.load(Ordering::Relaxed) {
            return;
        }

        let client_event = event.as_ref();

        let event = match serde_json::to_string(client_event) {
            Ok(event) => event,
            Err(err) => {
                tracing::warn!("ClientEvents: unable to serialize event. {err:?}");
                return;
            }
        };

        let serialized_details = match serde_json::to_value(details) {
            Ok(details) => details,
            Err(err) => {
                tracing::warn!("ClientEvents: unable to serialize details. {err:?}");
                return;
            }
        };

        let result = ClientEvents {
            created_at_ns: now_ns(),
            group_id,
            event,
            details: serialized_details,
        }
        .store(db);
        if let Err(err) = result {
            // We don't want ClientEvents causing any issues, so we just warn if something goes wrong.
            tracing::warn!("ClientEvents: {err:?}");
        }

        // Clear old events on build.
        if matches!(client_event, ClientEvent::ClientBuild) {
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

#[derive(Debug, Serialize)]
pub enum ClientEvent {
    ClientBuild,
    QueueIntent,
    EpochChange,
    GroupWelcome,
    GroupCreate,
    GroupMembershipChange,
    MsgStreamConnect,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Details {
    MsgStreamConnect {
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
    },
    QueueIntent {
        intent_kind: IntentKind,
    },
    GroupWelcome {
        conversation_type: ConversationType,
        added_by_inbox_id: String,
    },
    GroupCreate {
        conversation_type: ConversationType,
    },
    GroupMembershipChange {
        added: Vec<String>,
        removed: Vec<String>,
    },
    EpochChange {
        prev_epoch: i64,
        new_epoch: i64,
        cursor: i64,
        validated_commit: Option<String>,
    },
}

impl AsRef<ClientEvent> for ClientEvent {
    fn as_ref(&self) -> &ClientEvent {
        self
    }
}

#[cfg(test)]
mod tests {

    use std::collections::HashMap;

    use crate::{
        Store,
        client_events::{ClientEvent, ClientEvents, Details},
        group_intent::IntentKind,
        with_connection,
    };

    #[xmtp_common::test(unwrap_try = "true")]
    // A client build event should clear old events.
    async fn clear_old_events() {
        with_connection(|conn| {
            let details: HashMap<String, String> = HashMap::default();
            ClientEvents {
                created_at_ns: 0,
                group_id: None,
                event: serde_json::to_string(&ClientEvent::ClientBuild)?,
                details: serde_json::to_value(details.clone())?,
            }
            .store(conn)?;
            ClientEvents {
                created_at_ns: 0,
                group_id: None,
                event: serde_json::to_string(&ClientEvent::QueueIntent)?,
                details: serde_json::to_value(Details::QueueIntent {
                    intent_kind: IntentKind::KeyUpdate,
                })?,
            }
            .store(conn)?;

            let all = ClientEvents::all_events(conn)?;
            assert_eq!(all.len(), 2);

            ClientEvents::track(conn, None, ClientEvent::ClientBuild, Some(details));
            let all = ClientEvents::all_events(conn)?;
            assert_eq!(all.len(), 1);
        })
        .await;
    }
}
