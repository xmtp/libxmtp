use super::{
    ConnectionExt, DbConnection, consent_record::ConsentState, group::ConversationType,
    group_intent::IntentKind, schema::events::dsl,
};
use crate::{impl_store, schema::events};
use diesel::{Insertable, Queryable, associations::HasTable, prelude::*};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use xmtp_common::{NS_IN_30_DAYS, time::now_ns};

#[derive(Insertable, Queryable, Debug, Clone, Serialize)]
#[diesel(table_name = events)]
#[diesel(primary_key(created_at_ns))]
pub struct Events {
    pub created_at_ns: i64,
    pub group_id: Option<Vec<u8>>,
    pub event: String,
    pub details: serde_json::Value,
}

impl_store!(Events, events);

impl Events {
    pub fn clear_old_events<C: ConnectionExt>(
        db: &DbConnection<C>,
    ) -> Result<(), crate::ConnectionError> {
        db.raw_query_write(|db| {
            diesel::delete(dsl::events.filter(dsl::created_at_ns.lt(now_ns() - NS_IN_30_DAYS)))
                .execute(db)?;
            Ok(())
        })
    }

    pub fn all_events(db: &DbConnection) -> Result<Vec<Self>, crate::ConnectionError> {
        db.raw_query_read(|db| dsl::events.load(db))
    }

    pub fn all_events_paged<C: ConnectionExt>(
        db: &C,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, crate::ConnectionError> {
        let query = dsl::events::table()
            .order_by(dsl::created_at_ns.asc())
            .limit(limit)
            .offset(offset);
        db.raw_query_read(|db| query.load(db))
    }

    pub fn key_updates(db: &DbConnection) -> Result<Vec<Self>, crate::ConnectionError> {
        db.raw_query_read(|db| {
            let query = dsl::events.filter(diesel::dsl::sql::<diesel::sql_types::Bool>(
                "jsonb_extract(details, '$.QueueIntent.intent_kind') = 'KeyUpdate'",
            ));

            query.load::<Events>(db)
        })
    }
}

#[derive(Debug, Serialize)]
pub enum Event {
    ClientBuild,
    QueueIntent,
    EpochChange,
    GroupWelcome,
    GroupCreate,
    GroupMembershipChange,
    KPRotate,
    MsgStreamConnect,
    SyncGroupMsg,
    Error,
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
    KPRotate {
        history_id: i32,
    },
    Error {
        error: String,
    },
}

impl AsRef<Event> for Event {
    fn as_ref(&self) -> &Event {
        self
    }
}

#[cfg(test)]
mod tests {

    use std::collections::HashMap;

    use crate::{
        Store,
        events::{Details, Event, Events},
        group_intent::IntentKind,
        with_connection,
    };

    #[xmtp_common::test(unwrap_try = "true")]
    // A client build event should clear old events.
    async fn test_store_events() {
        with_connection(|conn| {
            let details: HashMap<String, String> = HashMap::default();
            Events {
                created_at_ns: 0,
                group_id: None,
                event: serde_json::to_string(&Event::ClientBuild)?,
                details: serde_json::to_value(details.clone())?,
            }
            .store(conn)?;
            Events {
                created_at_ns: 0,
                group_id: None,
                event: serde_json::to_string(&Event::QueueIntent)?,
                details: serde_json::to_value(Details::QueueIntent {
                    intent_kind: IntentKind::KeyUpdate,
                })?,
            }
            .store(conn)?;

            let all = Events::all_events(conn)?;
            assert_eq!(all.len(), 2);
        })
        .await;
    }
}
