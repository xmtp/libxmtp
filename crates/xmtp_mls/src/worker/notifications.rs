//! One durable task owns notification registration, renewal, and subscription sync.

use crate::{
    client::notifications::{
        NotificationConfig, NotificationError, decode, effective, encode, recipient_secret,
        stored_config,
    },
    context::XmtpSharedContext,
    groups::MlsGroup,
    state_tx::{state_write, state_write_with_events},
    worker::tasks::TaskOutcome,
};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use xmtp_common::{NS_IN_HOUR, NS_IN_SEC, RetryableError, time};
use xmtp_configuration::{
    NOTIFICATION_BATCH_TOPICS as REQUEST_TOPICS, NOTIFICATION_REQUEST_TIMEOUT,
};
use xmtp_db::{
    StorageError,
    TransactionOutcome::Continue,
    XmtpMlsStorageProvider,
    notifications::{StoredNotification, UploadedTopic},
    prelude::*,
    tasks::{NewTask, TaskDataHash, data_hash_for},
    user_preferences::{HmacKey, StoredUserPreferences},
};
use xmtp_events::{ClientEvent, EventWriter, NotificationsFailed};
use xmtp_proto::{
    backend_v1::{RecipientState, Subscription, UnregisterRequest, UpdateSubscriptionsRequest},
    types::{GroupId, Topic},
    xmtp::mls::database::{NotificationSync, Task, task::Task as TaskKind},
};

const RETRY_NS: i64 = 60 * NS_IN_SEC;

/// The two logical deadlines owned by the single durable task.
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Deadlines {
    pub(crate) sync_ns: i64,
    pub(crate) renewal_ns: i64,
    pub(crate) registration_owed: bool,
    /// A successful inline registration asks the runner to reset old backoff.
    #[serde(default)]
    reset_retry: bool,
    /// No notification request before this time: a rate-limit rejection's delay.
    #[serde(default)]
    rate_limited_until_ns: i64,
}

fn deadlines(record: &StoredNotification) -> Result<Deadlines, StorageError> {
    record
        .push_deadlines
        .as_deref()
        .map(decode)
        .transpose()
        .map(Option::unwrap_or_default)
}

fn task_proto() -> Task {
    Task {
        task: Some(TaskKind::NotificationSync(NotificationSync {})),
    }
}

pub(crate) fn task_hash() -> TaskDataHash {
    data_hash_for(&task_proto())
}

/// Only the runner changes its notification deadline. Producers send a memory hint.
pub(crate) fn wake<Context: XmtpSharedContext>(context: &Context) -> Result<(), StorageError> {
    state_write(context.mls_storage(), |tx| {
        let storage = tx.storage();
        let db = storage.db();
        let mut record = db.notification_record()?;
        if record.push_state != 1 {
            return Ok(Continue(()));
        }
        let now = time::now_ns();
        let seed = NewTask::builder()
            .originating_message_sequence_id(0)
            .expires_at_ns(i64::MAX)
            .max_attempts(i32::MAX)
            .backoff_scaling_factor(2.0)
            // The generic runner multiplies once before the first retry.
            .initial_backoff_duration_ns(RETRY_NS / 2)
            .max_backoff_duration_ns(NS_IN_HOUR)
            .next_attempt_at_ns(now)
            .build(task_proto())?;
        db.create_or_ignore_task(seed)?;
        let mut deadlines = deadlines(&record)?;
        if let Some(task) = db
            .get_tasks()?
            .into_iter()
            .find(|task| task.data_hash == task_hash().as_ref())
        {
            if deadlines.reset_retry {
                db.update_task(task.id, 0, task.last_attempted_at_ns, now)?;
                deadlines.reset_retry = false;
                record.push_deadlines = Some(encode(&deadlines)?);
                db.save_notification_record(&record)?;
            } else if task.attempts > 0 {
                // Ordinary wakes retain backoff. Only a confirmed inline Register
                // can request a reset, which the runner consumes in this transaction.
                return Ok(Continue(()));
            }
        }
        db.pull_in_task_deadline(&task_hash(), now)?;
        Ok::<_, StorageError>(Continue(()))
    })
    .map(|outcome| outcome.into_continued())
}

/// Request prompt sync after an inline Register confirmed this generation.
/// The task runner remains the sole owner of the task's retry state.
pub(crate) fn resume_after_registration<Context: XmtpSharedContext>(
    context: &Context,
    generation: i64,
) -> Result<(), StorageError> {
    state_write(context.mls_storage(), |tx| {
        let storage = tx.storage();
        let db = storage.db();
        let mut record = db.notification_record()?;
        if record.push_state == 1 && record.push_generation == generation {
            let mut deadlines = deadlines(&record)?;
            deadlines.reset_retry = true;
            record.push_deadlines = Some(encode(&deadlines)?);
            db.save_notification_record(&record)?;
        }
        Ok::<_, StorageError>(Continue(()))
    })
    .map(|outcome| outcome.into_continued())
}

/// Bound auth, retries, and transport together. No database writer is held here.
pub(crate) async fn bounded<T>(
    future: impl std::future::Future<Output = Result<T, xmtp_api::ApiError>>,
) -> Result<T, NotificationError> {
    time::timeout(NOTIFICATION_REQUEST_TIMEOUT, future)
        .await
        .map_err(|_| NotificationError::RequestTimeout)?
        .map_err(NotificationError::from)
}

/// Send Unregister while the caller holds the notification request lock.
/// On success or NOT_FOUND, clear the owed reset and uploaded set atomically.
pub(crate) async fn unregister<Context: XmtpSharedContext>(
    context: &Context,
    record: &StoredNotification,
) -> Result<(), NotificationError> {
    let request = UnregisterRequest {
        recipient_id: record.push_recipient_id.clone().unwrap_or_default(),
        recipient_secret: recipient_secret(record),
    };
    match bounded(context.api().unregister(request)).await {
        Ok(_) | Err(NotificationError::NotFound) => {
            state_write(context.mls_storage(), |tx| {
                let storage = tx.storage();
                let db = storage.db();
                let mut current = db.notification_record()?;
                db.clear_uploaded_topics()?;
                current.push_unregister_owed = false;
                db.save_notification_record(&current)?;
                Ok::<_, StorageError>(Continue(()))
            })?;
            Ok(())
        }
        Err(error) => Err(error),
    }
}

/// Register while the caller holds the notification request lock.
/// The owed reset and registration share one waiting budget.
/// Return true only when the current generation accepts the successful response.
pub(crate) async fn register<Context: XmtpSharedContext>(
    context: &Context,
    record: &StoredNotification,
    config: &NotificationConfig,
) -> Result<bool, NotificationError> {
    // implements: PUSH-240
    let result = time::timeout(NOTIFICATION_REQUEST_TIMEOUT, async {
        // implements: PUSH-265
        // The owed flag implies push_last_state is None, so run() reaches registration.
        if record.push_unregister_owed {
            if let Err(error) = unregister(context, record).await {
                return if record_error(context, record.push_generation, &error, None)? {
                    Err(error)
                } else {
                    Ok(false)
                };
            }
            // Local settings can change while the reset waits on the backend.
            let current = context.db().notification_record()?;
            if current.push_generation != record.push_generation || current.push_state != 1 {
                return Ok(false);
            }
        }
        match bounded(context.api().register(config.registration(record))).await {
            Ok(response) => Ok(confirm(
                context,
                record.push_generation,
                config,
                &response,
                &[],
                &[],
            )?),
            Err(error) => {
                if record_error(context, record.push_generation, &error, None)? {
                    Err(error)
                } else {
                    Ok(false)
                }
            }
        }
    })
    .await;
    match result {
        Ok(result) => result,
        Err(_) => {
            let error = NotificationError::RequestTimeout;
            if record_error(context, record.push_generation, &error, None)? {
                Err(error)
            } else {
                Ok(false)
            }
        }
    }
}

/// Preserve confirmed deltas, but apply response state only to its configuration.
pub(crate) fn confirm<Context: XmtpSharedContext>(
    context: &Context,
    generation: i64,
    config: &NotificationConfig,
    response: &RecipientState,
    adds: &[UploadedTopic],
    removes: &[Vec<u8>],
) -> Result<bool, StorageError> {
    state_write(context.mls_storage(), |tx| {
        let storage = tx.storage();
        let db = storage.db();
        let mut record = db.notification_record()?;
        if record.push_state != 1 {
            return Ok(Continue(false));
        }
        // The request lock orders backend mutations and their confirmations.
        // A newer enable can change local rules while this request is in flight.
        // Retain its confirmed delta so the next scan can remove obsolete topics.
        db.confirm_uploaded_topics(adds, removes)?;
        if record.push_generation != generation {
            return Ok(Continue(false));
        }
        if !adds.is_empty() || !removes.is_empty() {
            record.push_suppressed = None;
        }
        let uploaded = db.uploaded_topics()?;
        if response.channel != config.channel_id() {
            tracing::warn!("notification channel differs from the local configuration");
        }
        if record.push_repairing {
            // Do not compare counts on the response that finishes a repair pass.
            if !uploaded.iter().any(|row| row.stale) {
                record.push_repairing = false;
            }
        } else if response.topic_count != uploaded.len() as u64 {
            tracing::warn!("notification topic count differs; starting a repair pass");
            db.mark_uploaded_topics_stale()?;
            record.push_repairing = !uploaded.is_empty();
        }
        let now = time::now_ns();
        let next = Deadlines {
            sync_ns: now,
            renewal_ns: now.saturating_add(response.expires_at_ns.saturating_sub(now) / 4),
            registration_owed: false,
            reset_retry: false,
            rate_limited_until_ns: 0,
        };
        record.push_deadlines = Some(encode(&next)?);
        record.push_last_state = Some(response.encode_to_vec());
        db.save_notification_record(&record)?;
        Ok::<_, StorageError>(Continue(true))
    })
    .map(|outcome| outcome.into_continued())
}

/// Persist only typed terminal failures and repair/suppression state for this generation.
pub(crate) fn record_error<Context: XmtpSharedContext>(
    context: &Context,
    generation: i64,
    error: &NotificationError,
    desired_fingerprint: Option<&[u8]>,
) -> Result<bool, StorageError> {
    state_write_with_events(context.mls_storage(), context.events(), |tx, events| {
        let storage = tx.storage();
        let db = storage.db();
        let mut record = db.notification_record()?;
        if record.push_generation != generation || record.push_state != 1 {
            return Ok(Continue(false));
        }
        // A later request failure must retain its own backoff, even if the
        // runner has not consumed an earlier inline registration hint yet.
        let mut deadlines = deadlines(&record)?;
        deadlines.reset_retry = false;
        // implements: PUSH-267, RATE-041
        if let Some(delay) = error.rate_limit_delay() {
            let delay = i64::try_from(delay.as_nanos()).unwrap_or(i64::MAX);
            deadlines.rate_limited_until_ns = time::now_ns().saturating_add(delay);
        }
        record.push_deadlines = Some(encode(&deadlines)?);
        if let Some(failure) = error.failure() {
            record.push_state = 2;
            record.push_failed_error = Some(encode(&failure)?);
            events.emit(
                Some(ClientEvent::NotificationsFailed(NotificationsFailed {
                    cause: error.to_string(),
                })),
                None,
            );
        } else if matches!(error, NotificationError::NotFound) {
            db.clear_uploaded_topics()?;
            record.push_repairing = true;
            record.push_deadlines = Some(encode(&Deadlines {
                registration_owed: true,
                ..Default::default()
            })?);
        } else if matches!(error, NotificationError::ResourceExhausted) {
            record.push_suppressed = desired_fingerprint.map(<[u8]>::to_vec);
            tracing::warn!("notification adds suppressed until the desired set changes");
        }
        db.save_notification_record(&record)?;
        Ok::<_, StorageError>(Continue(true))
    })
    .map(|outcome| outcome.into_continued())
}

#[derive(Clone)]
pub(crate) struct DesiredTopic {
    pub(crate) group_id: Option<GroupId>,
    pub(crate) include_commits: bool,
}

type Desired = BTreeMap<Vec<u8>, DesiredTopic>;

fn desired<Context: XmtpSharedContext>(
    context: &Context,
    config: &NotificationConfig,
) -> Result<Desired, NotificationError> {
    let db = context.db();
    let mut desired = Desired::new();
    for row in db.notification_groups()? {
        let group = MlsGroup::new_from_arc(
            context.clone(),
            row.id,
            row.dm_id.clone(),
            row.conversation_type,
            row.created_at_ns,
        );
        if !group.is_active()? || !effective(config, &row, group.consent_state()?) {
            continue;
        }
        desired.insert(
            Topic::new_group_message(row.id).cloned_vec(),
            DesiredTopic {
                group_id: Some(row.id),
                include_commits: config.include_commits,
            },
        );
    }
    if config.include_welcomes {
        desired.insert(
            Topic::new_welcome_message(context.installation_id()).cloned_vec(),
            DesiredTopic {
                group_id: None,
                include_commits: false,
            },
        );
    }
    Ok(desired)
}

fn desired_fingerprint(desired: &Desired) -> Result<Vec<u8>, StorageError> {
    let values: Vec<_> = desired
        .iter()
        .map(|(topic, rule)| (topic, rule.include_commits))
        .collect();
    Ok(xmtp_common::sha256_array(&encode(&values)?).to_vec())
}

/// Missing topics precede stale topics so a count mismatch cannot delay new subscriptions.
pub(crate) fn diff(
    desired: &Desired,
    uploaded: &[UploadedTopic],
    epoch: i64,
    fingerprint: &[u8],
    suppress_adds: bool,
) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let uploaded: BTreeMap<_, _> = uploaded
        .iter()
        .map(|row| (row.topic.as_slice(), row))
        .collect();
    let removes: Vec<_> = uploaded
        .keys()
        .filter(|topic| !desired.contains_key(**topic))
        .take(REQUEST_TOPICS)
        .map(|topic| topic.to_vec())
        .collect();
    let mut adds = Vec::new();
    if !suppress_adds {
        for (topic, _) in desired
            .iter()
            .filter(|(topic, _)| !uploaded.contains_key(topic.as_slice()))
        {
            if adds.len() + removes.len() == REQUEST_TOPICS {
                break;
            }
            adds.push(topic.clone());
        }
        for (topic, rule) in desired {
            if adds.len() + removes.len() == REQUEST_TOPICS {
                break;
            }
            let Some(row) = uploaded.get(topic.as_slice()) else {
                continue;
            };
            let keys_outdated = rule.group_id.is_some()
                && (row
                    .hmac_epoch_base
                    .is_none_or(|base| base > epoch + 1 || base.saturating_add(2) < epoch + 1)
                    || row.root_key_fingerprint != fingerprint);
            if row.stale || row.include_commits != rule.include_commits || keys_outdated {
                adds.push(topic.clone());
            }
        }
    }
    (adds, removes)
}

/// A subscription update and the uploads it confirms. The request holds the
/// recipient secret and derived HMAC keys, and wipes them on drop.
struct Batch {
    request: UpdateSubscriptionsRequest,
    uploaded: Vec<UploadedTopic>,
    fingerprint: Vec<u8>,
}

fn prepare_batch<Context: XmtpSharedContext>(
    context: &Context,
    record: &StoredNotification,
    desired: &Desired,
) -> Result<Batch, NotificationError> {
    let fingerprint = desired_fingerprint(desired)?;
    state_write(context.mls_storage(), |tx| {
        let storage = tx.storage();
        let db = storage.db();
        let root_fingerprint = match StoredUserPreferences::load(&db)?.hmac_key {
            Some(key) => xmtp_common::sha256_array(key.as_slice()).to_vec(),
            None if desired.values().any(|topic| topic.group_id.is_some()) => {
                let key = HmacKey::random_key();
                StoredUserPreferences::store_hmac_key(&db, key.as_slice(), None)?;
                xmtp_common::sha256_array(key.as_slice()).to_vec()
            }
            None => Vec::new(),
        };
        let epoch = crate::utils::time::hmac_epoch();
        let (adds, removes) = diff(
            desired,
            &db.uploaded_topics()?,
            epoch,
            &root_fingerprint,
            record.push_suppressed.as_deref() == Some(fingerprint.as_slice()),
        );
        // Built first, so an error below drops a batch that wipes its secrets.
        let mut batch = Batch {
            request: UpdateSubscriptionsRequest {
                recipient_id: record.push_recipient_id.clone().unwrap_or_default(),
                recipient_secret: recipient_secret(record),
                adds: Vec::with_capacity(adds.len()),
                removes,
            },
            uploaded: Vec::with_capacity(adds.len()),
            fingerprint,
        };
        for topic in adds {
            let rule = &desired[&topic];
            let mut subscription = Subscription {
                topic: topic.clone(),
                hmac_epoch_base: 0,
                hmac_keys: Vec::new(),
                include_commits: rule.include_commits,
            };
            let (hmac_epoch_base, key_fingerprint) = if let Some(group_id) = rule.group_id {
                let row = db
                    .find_group(&group_id)?
                    .ok_or(xmtp_db::NotFound::GroupById(group_id))?;
                let group = MlsGroup::new_from_arc(
                    context.clone(),
                    row.id,
                    row.dm_id,
                    row.conversation_type,
                    row.created_at_ns,
                );
                let keys = group.hmac_keys_in(&db, -1..=1)?;
                subscription.hmac_epoch_base =
                    keys.first().map(|key| key.epoch).unwrap_or(epoch - 1);
                // The request goes to the transport, which holds these copies.
                subscription.hmac_keys = keys.iter().map(|key| key.key.to_vec()).collect();
                (Some(subscription.hmac_epoch_base), root_fingerprint.clone())
            } else {
                (None, Vec::new())
            };
            batch.uploaded.push(UploadedTopic {
                topic,
                hmac_epoch_base,
                include_commits: rule.include_commits,
                root_key_fingerprint: key_fingerprint,
                stale: false,
            });
            batch.request.adds.push(subscription);
        }
        Ok::<_, StorageError>(Continue(batch))
    })
    .map(|outcome| outcome.into_continued())
    .map_err(NotificationError::from)
}

/// Execute one bounded notification turn. A busy inline request never blocks the task runner.
pub(crate) async fn run<Context: XmtpSharedContext>(
    context: &Context,
) -> Result<TaskOutcome, NotificationError> {
    let record = context.db().notification_record()?;
    if record.push_state != 1 {
        return Ok(TaskOutcome::Done);
    }
    let config = stored_config(&record)?;
    let deadlines = deadlines(&record)?;
    // A wake or an inline request cannot pull a request inside a rate-limit delay.
    if deadlines.rate_limited_until_ns > time::now_ns() {
        return Ok(TaskOutcome::RescheduleAt(deadlines.rate_limited_until_ns));
    }
    if record.push_last_state.is_none()
        || deadlines.registration_owed
        || deadlines.renewal_ns <= time::now_ns()
    {
        let Ok(_guard) = context.task_channels().notification_request.try_lock() else {
            return Ok(TaskOutcome::RescheduleAt(time::now_ns() + RETRY_NS));
        };
        let current = context.db().notification_record()?;
        if current.push_generation != record.push_generation || current.push_state != 1 {
            return finish_turn(context, Ok(()));
        }
        return finish_turn(
            context,
            register(context, &current, &config).await.map(|_| ()),
        );
    }
    let revision = context.task_channels().notification_revision();
    let desired = desired(context, &config)?;
    let mut batch = prepare_batch(context, &record, &desired)?;
    #[cfg(test)]
    tests::support::before_request();
    if context.task_channels().notification_revision() != revision {
        return finish_turn(context, Ok(()));
    }
    if batch.request.adds.is_empty() && batch.request.removes.is_empty() {
        let next = time::now_ns()
            .saturating_add(NS_IN_HOUR)
            .min(deadlines.renewal_ns);
        state_write(context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let mut current = db.notification_record()?;
            if current.push_generation == record.push_generation {
                current.push_deadlines = Some(encode(&Deadlines {
                    sync_ns: next,
                    ..deadlines
                })?);
                db.save_notification_record(&current)?;
            }
            Ok::<_, StorageError>(Continue(()))
        })?;
        return Ok(TaskOutcome::RescheduleAt(next));
    }
    let Ok(_guard) = context.task_channels().notification_request.try_lock() else {
        return Ok(TaskOutcome::RescheduleAt(time::now_ns() + RETRY_NS));
    };
    let current = context.db().notification_record()?;
    if current.push_generation != record.push_generation
        || current.push_state != 1
        || context.task_channels().notification_revision() != revision
    {
        return finish_turn(context, Ok(()));
    }
    // The request moves to the transport, which owns its copies of the
    // secrets. A retry builds a new batch from the stored state.
    let request = std::mem::take(&mut batch.request);
    let removes = request.removes.clone();
    let result = match bounded(context.api().update_subscriptions(request)).await {
        Ok(response) => {
            confirm(
                context,
                record.push_generation,
                &config,
                &response,
                &batch.uploaded,
                &removes,
            )?;
            Ok(())
        }
        Err(error) => {
            if record_error(
                context,
                record.push_generation,
                &error,
                Some(&batch.fingerprint),
            )? {
                Err(error)
            } else {
                Ok(())
            }
        }
    };
    finish_turn(context, result)
}

fn finish_turn<Context: XmtpSharedContext>(
    context: &Context,
    result: Result<(), NotificationError>,
) -> Result<TaskOutcome, NotificationError> {
    let record = context.db().notification_record()?;
    if record.push_state != 1 {
        return Ok(TaskOutcome::Done);
    }
    match result {
        Ok(()) | Err(NotificationError::NotFound) => Ok(TaskOutcome::RescheduleAt(time::now_ns())),
        Err(NotificationError::ResourceExhausted) if record.push_suppressed.is_some() => {
            Ok(TaskOutcome::RescheduleAt(time::now_ns()))
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
pub(crate) mod tests;
