//! Durable sync tasks that Welcome processing and device sync enqueue
//! inside their own storage transactions.

use crate::core::events::InternalEvent;
use xmtp_common::{NS_IN_DAY, time::now_ns};
use xmtp_configuration::SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS;
use xmtp_db::tasks::NewTask;
use xmtp_db::{StorageError, group::ConversationType, prelude::*};
use xmtp_events::EventWriter;
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::mls::database::{
    AddMissingInstallations as AddMissingInstallationsProto, Task as TaskProto,
    task::Task as TaskKindProto,
};

/// Maximum duplicate groups per DM scheduled in addition to its winner.
const MAX_DM_EXTRA_GROUPS: usize = 10;

/// A new sync group from a Welcome means a new installation: schedule one
/// durable task per eligible group, with bounded DM extras (deduped by
/// payload hash), in the transaction that installs the Welcome, so a restart
/// cannot lose them.
// implements: SYNC-014
pub(crate) fn schedule_newcomer_adds(
    db: &impl DbQuery,
    events: &impl EventWriter<InternalEvent>,
) -> Result<usize, StorageError> {
    use xmtp_db::consent_record::ConsentState;
    use xmtp_db::diesel::{RunQueryDsl, sql_query, sql_types::*};
    use xmtp_db::group::{GroupMembershipState::*, StoredGroup, consent_join_sql};

    let consent = [ConsentState::Allowed, ConsentState::Unknown].map(|s| s as i32);
    let membership = [Allowed, Rejected, Pending, PendingRemove].map(|s| s as i32);
    let types = ConversationType::virtual_types();
    let virtuals: Vec<_> = types.into_iter().map(|t| t as i32).collect();
    // implements: ARCH-015
    let groups = db.raw_query(|conn| {
        sql_query(format!(
            "WITH eligible AS (
              SELECT g.id, g.dm_id, g.last_message_ns, g.created_at_ns FROM groups g LEFT JOIN consent_records ON {}
              WHERE (state IS NULL OR state IN (SELECT value FROM json_each(?))) AND COALESCE(last_message_ns, created_at_ns) > ?
                AND membership_state IN (SELECT value FROM json_each(?)) AND conversation_type NOT IN (SELECT value FROM json_each(?))
            ), winners AS (
              SELECT *, ROW_NUMBER() OVER (PARTITION BY dm_id ORDER BY
                COALESCE(last_message_ns, 0) DESC, id DESC) AS winner_rank FROM eligible WHERE dm_id IS NOT NULL
            ), ranked AS (
              SELECT id, ROW_NUMBER() OVER (PARTITION BY dm_id ORDER BY
                winner_rank = 1 DESC, COALESCE(last_message_ns, created_at_ns) DESC, id DESC) AS rank FROM winners
            ) SELECT * FROM groups WHERE id IN (SELECT id FROM eligible WHERE dm_id IS NULL
              UNION ALL SELECT id FROM ranked WHERE rank <= ?)",
            consent_join_sql("g"),
        ))
        .bind::<Text, _>(serde_json::json!(consent).to_string())
        .bind::<BigInt, _>(now_ns() - NS_IN_DAY * 90)
        .bind::<Text, _>(serde_json::json!(membership).to_string())
        .bind::<Text, _>(serde_json::json!(virtuals).to_string())
        .bind::<BigInt, _>((MAX_DM_EXTRA_GROUPS + 1) as i64)
        .load::<StoredGroup>(conn)
    })?;
    for group in &groups {
        schedule_add_missing_installations(db, events, group.id, None, false)?;
    }
    Ok(groups.len())
}

/// Durably enqueue one AddMissingInstallations task for `group_id` (deduped
/// by payload hash against any pending row for the same group) and wake the
/// TaskRunner. The task is due once `delay`, a rate-limit delay, has passed.
/// A task `until_done` never expires, and a failing one backs off to at most
/// one run an interval.
pub(crate) fn schedule_add_missing_installations(
    db: &impl DbQuery,
    events: &impl EventWriter<InternalEvent>,
    group_id: GroupId,
    delay: Option<xmtp_common::time::Duration>,
    until_done: bool,
) -> Result<(), StorageError> {
    let due = now_ns().saturating_add(delay.map_or(0, |delay| {
        i64::try_from(delay.as_nanos()).unwrap_or(i64::MAX)
    }));
    let mut task = NewTask::builder();
    task.originating_message_sequence_id(0)
        .next_attempt_at_ns(due);
    if until_done {
        task.expires_at_ns(i64::MAX)
            .max_attempts(i32::MAX)
            .max_backoff_duration_ns(SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS);
    } else {
        // The usual three days to succeed count from when the task is due.
        task.expires_at_ns(due.saturating_add(NS_IN_DAY * 3));
    }
    let task = task.build(TaskProto {
        task: Some(TaskKindProto::AddMissingInstallations(
            AddMissingInstallationsProto {
                group_id: group_id.to_vec(),
            },
        )),
    })?;
    let hash = task.data_hash.clone();
    db.create_or_ignore_task(task)?;
    if until_done {
        // A row an older build stored for this group is kept instead of the
        // new one, so it must not expire either.
        use xmtp_db::{diesel, diesel::prelude::*, schema::tasks::dsl};
        db.raw_query(|c| {
            diesel::update(dsl::tasks.filter(dsl::data_hash.eq(&hash)))
                .set((
                    dsl::expires_at_ns.eq(i64::MAX),
                    dsl::max_attempts.eq(i32::MAX),
                    dsl::max_backoff_duration_ns.eq(SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS),
                    dsl::next_attempt_at_ns.eq(diesel::dsl::sql::<diesel::sql_types::BigInt>(
                        "MAX(next_attempt_at_ns, ",
                    )
                    .bind::<diesel::sql_types::BigInt, _>(due)
                    .sql(")")),
                ))
                .execute(c)
        })?;
    }
    events.emit(None, Some(InternalEvent::TaskScheduled));
    Ok(())
}
