use super::{
    ConnectionExt, DbConnection, consent_record::ConsentState, group::ConversationType,
    group_intent::IntentKind, schema::events::dsl,
};
use crate::{Store, impl_store, schema::events};
use diesel::{
    Insertable, Queryable,
    associations::HasTable,
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
    sqlite::Sqlite,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use xmtp_common::{NS_IN_30_DAYS, time::now_ns};

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = events)]
#[diesel(primary_key(created_at_ns))]
pub struct Events {
    pub created_at_ns: i64,
    pub group_id: Option<Vec<u8>>,
    pub event: String,
    pub details: serde_json::Value,
    pub level: EventLevel,
    pub icon: Option<String>,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
/// The state of the consent
pub enum EventLevel {
    // Just run-of-the-mill info (no border on dashboard)
    None = 0,
    // green border on dashboard
    Success = 1,
    // orange border on dashboard
    Warn = 2,
    // red border on dashboard
    Error = 3,
    // Irrecoverable error - purple border on dashboard
    Fault = 4,
}

impl ToSql<Integer, Sqlite> for EventLevel
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for EventLevel
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            0 => Ok(EventLevel::None),
            1 => Ok(EventLevel::Success),
            2 => Ok(EventLevel::Warn),
            3 => Ok(EventLevel::Error),
            4 => Ok(EventLevel::Fault),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

impl_store!(Events, events);

pub static EVENTS_ENABLED: AtomicBool = AtomicBool::new(true);

impl Events {
    #[allow(invalid_type_param_default)]
    pub fn track<C: ConnectionExt>(
        db: &DbConnection<C>,
        group_id: Option<Vec<u8>>,
        event: impl AsRef<str>,
        details: impl Serialize,
        icon: Option<String>,
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

        let result = Events {
            created_at_ns: now_ns(),
            group_id,
            event,
            details: serialized_details,
            level: EventLevel::None,
            icon,
        }
        .store(db);
        if let Err(err) = result {
            // We don't want ClientEvents causing any issues, so we just warn if something goes wrong.
            tracing::warn!("ClientEvents: {err:?}");
        }

        // Clear old events on build.
        if client_event == "Client Build" {
            if let Err(err) = Self::clear_old_events(db) {
                tracing::warn!("ClientEvents clear old events: {err:?}");
            }
        }
    }

    fn clear_old_events<C: ConnectionExt>(
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
                "jsonb_extract(details, '$.intent_kind') = 'KeyUpdate'",
            ));

            query.load::<Events>(db)
        })
    }
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

#[cfg(test)]
mod tests {

    use std::collections::HashMap;

    use crate::{
        Store,
        events::{Details, EventLevel, Events},
        group_intent::IntentKind,
        with_connection,
    };

    #[xmtp_common::test(unwrap_try = "true")]
    // A client build event should clear old events.
    async fn clear_old_events() {
        with_connection(|conn| {
            let details: HashMap<String, String> = HashMap::default();
            Events {
                created_at_ns: 0,
                group_id: None,
                event: "Queue Intent".to_string(),
                details: serde_json::to_value(details.clone())?,
                level: EventLevel::None,
                icon: None,
            }
            .store(conn)?;
            Events {
                created_at_ns: 0,
                group_id: None,
                event: "Queue Intent".to_string(),
                details: serde_json::to_value(Details::QueueIntent {
                    intent_kind: IntentKind::KeyUpdate,
                })?,
                level: EventLevel::None,
                icon: None,
            }
            .store(conn)?;

            let all = Events::all_events(conn)?;
            assert_eq!(all.len(), 2);

            Events::track(conn, None, "Client Build", Some(details), None);
            let all = Events::all_events(conn)?;
            assert_eq!(all.len(), 1);
        })
        .await;
    }
}
