//! Durable sync tasks that Welcome processing and device sync enqueue
//! inside their own storage transactions.

use crate::core::events::InternalEvent;
use xmtp_common::{NS_IN_DAY, time::now_ns};
use xmtp_configuration::SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS;
use xmtp_db::tasks::NewTask;
use xmtp_db::{
    StorageError,
    consent_record::ConsentState,
    group::{GroupMembershipState, GroupQueryArgs},
    prelude::*,
};
use xmtp_events::EventWriter;
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::mls::database::{
    AddMissingInstallations as AddMissingInstallationsProto, Task as TaskProto,
    task::Task as TaskKindProto,
};

/// A new sync group from a Welcome means a new installation: schedule one
/// durable AddMissingInstallations task per eligible group (deduped by
/// payload hash), in the transaction that installs the Welcome, so a restart
/// cannot lose them.
// implements: SYNC-014
pub(crate) fn schedule_newcomer_adds(
    db: &impl DbQuery,
    events: &impl EventWriter<InternalEvent>,
) -> Result<usize, StorageError> {
    let groups = db.find_groups(GroupQueryArgs {
        last_activity_after_ns: Some(now_ns() - NS_IN_DAY * 90),
        consent_states: Some(vec![ConsentState::Allowed, ConsentState::Unknown]),
        // A Restored placeholder takes no commit until a Welcome activates
        // it, so its task could never finish.
        // implements: ARCH-015
        allowed_states: Some(vec![
            GroupMembershipState::Allowed,
            GroupMembershipState::Rejected,
            GroupMembershipState::Pending,
            GroupMembershipState::PendingRemove,
        ]),
        ..Default::default()
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
                ))
                .execute(c)
        })?;
    }
    events.emit(None, Some(InternalEvent::TaskScheduled));
    Ok(())
}
