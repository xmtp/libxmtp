//! Fixed processing targets over the shared durable receiver.

#[cfg(test)]
mod tests;

use futures::{StreamExt, stream};
use std::{collections::HashSet, sync::Arc};
use xmtp_common::{
    ErrorCode, RetryableError,
    time::{Duration, Instant, sleep},
};
use xmtp_db::{
    StorageError,
    consent_record::ConsentState,
    group::GroupQueryArgs,
    incoming_envelope::{NetworkEntityKind, QueryIncomingEnvelope, StreamTopic, TopicProgress},
    prelude::*,
};
use xmtp_proto::types::{Cursor, GroupId, Topic, TopicCursor, TopicKind};

use super::incoming::{
    IncomingCoordinator, IncomingError, IncomingProcessing, IncomingReceivePolicy, IncomingScope,
};
use crate::{
    context::XmtpSharedContext,
    groups::{GroupError, MlsGroup},
};

/// Why one fixed processing obligation is unfinished.
#[derive(Debug, Clone)]
pub enum BarrierCause {
    /// The backend head has not been captured; zero must not be assumed.
    TargetPending,
    /// The durable received prefix has not reached the target.
    ReceiptPending,
    /// Receipt is complete, but ordered processing is still pending.
    ProcessingPending,
    Blocked(String),
    Storage(Arc<StorageError>),
    Receiver(Arc<IncomingError>),
    InvalidTopic,
}

/// One fixed target and its latest durable progress (STR-042 and STR-043).
#[derive(Debug, Clone)]
pub struct BarrierTopic {
    /// The group, Welcome, or identity topic covered by this obligation.
    pub topic: Topic,
    /// Fixed backend head H. `None` means target capture did not succeed.
    pub target: Option<Cursor>,
    /// Durable received prefix F, including retained pending envelopes.
    pub received: Cursor,
    /// Durable processed prefix P; independent of application delivery.
    pub processed: Cursor,
    /// Unresolved Welcome sequence IDs at or below H, even after later successes.
    pub unresolved_welcomes: Vec<Cursor>,
    /// Removal completed this obligation without advancing P further.
    pub inactive: bool,
    /// `None` only when this obligation is complete.
    pub cause: Option<BarrierCause>,
}

impl BarrierTopic {
    /// Receipt alone does not complete a processing obligation.
    pub fn complete(&self) -> bool {
        self.cause.is_none()
    }

    fn blocked(&self) -> bool {
        matches!(
            self.cause,
            Some(BarrierCause::Blocked(_) | BarrierCause::InvalidTopic)
        ) || matches!(&self.cause, Some(BarrierCause::Storage(error)) if !error.is_retryable())
            || matches!(&self.cause, Some(BarrierCause::Receiver(error)) if !error.is_retryable())
    }
}

/// Settled obligations from one bounded sync run.
#[derive(Debug, Clone)]
pub struct BarrierSnapshot {
    /// Each topic keeps its own fixed target and progress.
    pub topics: Vec<BarrierTopic>,
}

/// Why a bounded sync stopped before every obligation completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarrierFailure {
    Blocked,
    Deadline,
    Cancelled,
}

#[derive(Debug, thiserror::Error, ErrorCode)]
pub enum BarrierError {
    /// Processing did not meet the fixed targets. Pending work remains durable. May be retryable.
    #[error("Processing barrier did not complete: {reason:?}; {} unfinished topics", unfinished.len())]
    Incomplete {
        /// The run-level stop reason; individual causes remain below.
        reason: BarrierFailure,
        /// Every unfinished obligation, not only the first failure.
        unfinished: Vec<BarrierTopic>,
    },
}

impl RetryableError for BarrierError {
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Incomplete {
                reason: BarrierFailure::Deadline,
                ..
            }
        )
    }
}

impl crate::worker::NeedsDbReconnect for BarrierError {
    fn needs_db_reconnect(&self) -> bool {
        let Self::Incomplete { unfinished, .. } = self;
        unfinished.iter().any(|topic| match &topic.cause {
            Some(BarrierCause::Storage(error)) => error.db_needs_connection(),
            Some(BarrierCause::Receiver(error)) => error.needs_db_reconnect(),
            _ => false,
        })
    }
}

/// Test durable completion without assuming transport coverage or an active group.
/// Welcome progress requires every pending parent through the fixed target to finish.
pub(crate) fn durable_complete(
    kind: NetworkEntityKind,
    target: Option<Cursor>,
    progress: TopicProgress,
    pending_count: usize,
) -> bool {
    target.is_some_and(|target| {
        progress.received >= target
            && match kind {
                NetworkEntityKind::Welcome => pending_count == 0,
                NetworkEntityKind::Group | NetworkEntityKind::Identity => {
                    progress.processed >= target
                }
            }
    })
}

fn read_topic<C: XmtpSharedContext>(context: &C, topic: &Topic, target: Cursor) -> BarrierTopic {
    let mut status = BarrierTopic {
        topic: topic.clone(),
        target: Some(target),
        received: Cursor::default(),
        processed: Cursor::default(),
        unresolved_welcomes: Vec::new(),
        inactive: false,
        cause: Some(BarrierCause::ReceiptPending),
    };
    let kind = match topic.kind() {
        TopicKind::GroupMessagesV1 => NetworkEntityKind::Group,
        TopicKind::WelcomeMessagesV1 => NetworkEntityKind::Welcome,
        TopicKind::IdentityUpdatesV1 => NetworkEntityKind::Identity,
        _ => {
            status.cause = Some(BarrierCause::InvalidTopic);
            return status;
        }
    };
    let db_topic = StreamTopic {
        entity_id: topic.identifier().to_vec(),
        kind,
    };
    let db = context.db();
    let progress = match db.topic_progress(&db_topic) {
        Ok(progress) => progress,
        Err(error) => {
            status.cause = Some(BarrierCause::Storage(Arc::new(error)));
            return status;
        }
    };
    status.received = progress.received;
    status.processed = progress.processed;
    if kind == NetworkEntityKind::Group {
        match GroupId::try_from(topic.identifier()) {
            Ok(id) => match MlsGroup::new_cached(context.context_ref().clone(), &id)
                .map_err(GroupError::from)
                .and_then(|(group, _)| group.is_active())
            {
                Ok(false) => {
                    status.inactive = true;
                    status.cause = None;
                    return status;
                }
                Ok(true) => {}
                Err(error) => {
                    status.cause = Some(BarrierCause::Receiver(Arc::new(IncomingError::Group(
                        error,
                    ))));
                    return status;
                }
            },
            Err(_) => {
                status.cause = Some(BarrierCause::InvalidTopic);
                return status;
            }
        }
    }
    let pending = match db.pending_states_through(&db_topic, target) {
        Ok(pending) => pending,
        Err(error) => {
            status.cause = Some(BarrierCause::Storage(Arc::new(error)));
            return status;
        }
    };
    if kind == NetworkEntityKind::Welcome {
        status.unresolved_welcomes = pending.iter().map(|row| row.sequence_id).collect();
    }
    status.cause = if progress.received < target {
        Some(BarrierCause::ReceiptPending)
    } else if durable_complete(kind, Some(target), progress, pending.len()) {
        None
    } else if !pending.is_empty()
        && (kind != NetworkEntityKind::Welcome && pending[0].blocked
            || kind == NetworkEntityKind::Welcome && pending.iter().all(|row| row.blocked))
    {
        Some(BarrierCause::Blocked(
            pending[0]
                .error_code
                .clone()
                .unwrap_or_else(|| "processing_blocked".into()),
        ))
    } else {
        Some(BarrierCause::ProcessingPending)
    };
    status
}

/// Sample targets once and query missing receipt without first waiting for a live stream.
/// Traffic after these targets cannot extend the call.
pub async fn receive_through_current<C: XmtpSharedContext>(
    context: &C,
    topics: Vec<Topic>,
) -> Result<BarrierSnapshot, GroupError> {
    let deadline = Instant::now() + context.stream_settings().barrier_timeout;
    Ok(receive_through_current_until(context, topics, deadline).await?)
}

/// Target sampling and processing use one deadline.
pub async fn receive_through_current_until<C: XmtpSharedContext>(
    context: &C,
    topics: Vec<Topic>,
    deadline: Instant,
) -> Result<BarrierSnapshot, BarrierError> {
    let coordinator = IncomingCoordinator::for_context(context);
    let _receipt = coordinator.acquire(IncomingScope::Topics(topics.clone()));
    let (targets, unavailable) = capture_targets(context, topics, deadline).await;
    wait_for_targets(
        context,
        targets,
        unavailable,
        None,
        IncomingReceivePolicy::ImmediateQuery,
        deadline,
        &mut HashSet::new(),
    )
    .await
}

/// Fixed starting groups plus discoveries attributed to Welcomes at or below the fixed H.
pub struct GroupBarrierRun {
    /// Exact enrolled scope; excludes unrelated groups created while the call runs.
    pub group_ids: Vec<GroupId>,
    /// Successful targets or every unfinished obligation on failure.
    pub result: Result<BarrierSnapshot, BarrierError>,
}

/// Freezes which Welcome discoveries may expand an all-groups sync run.
struct WelcomeDiscovery {
    topic: Topic,
    target: Cursor,
    consent_states: Option<Vec<ConsentState>>,
}

/// Process starting groups and enrolled Welcome discoveries under one deadline.
/// Later local groups and Welcomes above the sampled head cannot extend the run.
pub async fn receive_with_welcomes_until<C: XmtpSharedContext>(
    context: &C,
    starting_groups: Vec<GroupId>,
    consent_states: Option<Vec<ConsentState>>,
    deadline: Instant,
) -> GroupBarrierRun {
    let mut groups: HashSet<_> = starting_groups.into_iter().collect();
    let welcome_topic = Topic::new_welcome_message(context.installation_id());
    let mut topics: Vec<_> = groups.iter().map(Topic::new_group_message).collect();
    topics.push(welcome_topic.clone());
    let coordinator = IncomingCoordinator::for_context(context);
    let _receipt = coordinator.acquire(IncomingScope::Topics(topics.clone()));
    let (targets, unavailable) = capture_targets(context, topics, deadline).await;
    let discovery = targets
        .get(&welcome_topic)
        .copied()
        .map(|target| WelcomeDiscovery {
            topic: welcome_topic,
            target,
            consent_states,
        });
    let result = wait_for_targets(
        context,
        targets,
        unavailable,
        discovery,
        IncomingReceivePolicy::ImmediateQuery,
        deadline,
        &mut groups,
    )
    .await;
    let mut group_ids: Vec<_> = groups.into_iter().collect();
    group_ids.sort();
    GroupBarrierRun { group_ids, result }
}

/// Settle all bounded head queries; keep successful targets when another query fails.
async fn capture_targets<C: XmtpSharedContext>(
    context: &C,
    topics: Vec<Topic>,
    deadline: Instant,
) -> (TopicCursor, Vec<BarrierTopic>) {
    let mut topics = topics;
    topics.sort_by_key(Topic::cloned_vec);
    topics.dedup();
    let chunks: Vec<_> = topics
        .chunks(xmtp_configuration::BACKEND_DEFAULT_MAX_QUERY_TOPICS)
        .map(<[Topic]>::to_vec)
        .collect();
    let results = stream::iter(chunks)
        .map(|topics| async move {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let result = xmtp_common::time::timeout(
                remaining,
                context.api().newest_topic_cursors(topics.clone()),
            )
            .await;
            (topics, result)
        })
        .buffer_unordered(context.stream_settings().max_dependency_requests)
        .collect::<Vec<_>>()
        .await;
    let mut targets = TopicCursor::new();
    let mut unavailable = Vec::new();
    for (topics, result) in results {
        let cause = match result {
            Ok(Ok(captured)) => {
                targets.extend(captured);
                continue;
            }
            Ok(Err(error)) => BarrierCause::Receiver(Arc::new(IncomingError::Transport(
                xmtp_proto::api::NetworkError::new(error),
            ))),
            Err(_) => BarrierCause::TargetPending,
        };
        for topic in topics {
            let mut status = read_topic(context, &topic, Cursor::default());
            status.target = None;
            status.cause = Some(cause.clone());
            unavailable.push(status);
        }
    }
    (targets, unavailable)
}

/// Wait for all independent obligations before reporting blocked work.
/// Prefer a healthy receiver for caller-supplied targets, with bounded Query fallback.
pub async fn wait_through<C: XmtpSharedContext>(
    context: &C,
    targets: TopicCursor,
    timeout: Option<Duration>,
) -> Result<BarrierSnapshot, BarrierError> {
    let deadline = Instant::now() + timeout.unwrap_or(context.stream_settings().barrier_timeout);
    wait_for_targets(
        context,
        targets,
        Vec::new(),
        None,
        IncomingReceivePolicy::StreamFirst,
        deadline,
        &mut HashSet::new(),
    )
    .await
}

async fn wait_for_targets<C: XmtpSharedContext>(
    context: &C,
    mut targets: TopicCursor,
    mut unavailable: Vec<BarrierTopic>,
    discovery: Option<WelcomeDiscovery>,
    receive_policy: IncomingReceivePolicy,
    deadline: Instant,
    groups: &mut HashSet<GroupId>,
) -> Result<BarrierSnapshot, BarrierError> {
    let coordinator = IncomingCoordinator::for_context(context);
    let lease = coordinator.acquire(IncomingScope::Barrier {
        targets: targets.clone(),
        deadline,
        receive_policy,
    });
    loop {
        #[cfg(test)]
        tests::before_progress_read();
        // Read completion before discovery. A completed Welcome transaction also
        // records its groups, so the later discovery scan must include them.
        let mut topics: Vec<_> = targets
            .iter()
            .map(|(topic, target)| read_topic(context, topic, *target))
            .collect();
        let mut discovery_failure = None;
        if let Some(discovery) = &discovery {
            let discovered = (|| -> Result<Vec<GroupId>, StorageError> {
                let db = context.db();
                let eligible = db.group_ids_discovered_through(discovery.target)?;
                let selected: HashSet<_> = db
                    .find_groups(GroupQueryArgs {
                        consent_states: discovery.consent_states.clone(),
                        include_sync_groups: true,
                        include_duplicate_dms: true,
                        ..Default::default()
                    })?
                    .into_iter()
                    .map(|group| group.id)
                    .collect();
                Ok(eligible
                    .into_iter()
                    .filter(|id| selected.contains(id))
                    .collect())
            })();
            match discovered {
                Ok(discovered) => {
                    let new_topics: Vec<_> = discovered
                        .into_iter()
                        .filter(|id| groups.insert(*id))
                        .map(Topic::new_group_message)
                        .collect();
                    if !new_topics.is_empty() {
                        let (captured, failures) =
                            capture_targets(context, new_topics, deadline).await;
                        targets.extend(captured);
                        unavailable.extend(failures);
                        lease.replace_scope(IncomingScope::Barrier {
                            targets: targets.clone(),
                            deadline,
                            receive_policy,
                        });
                        // The prior snapshot did not include these obligations.
                        continue;
                    }
                }
                Err(error) => {
                    let mut status = read_topic(context, &discovery.topic, discovery.target);
                    status.cause = Some(BarrierCause::Storage(Arc::new(error)));
                    discovery_failure = Some(status);
                }
            }
        }
        let receiver = lease.snapshot();
        for topic in &mut topics {
            if topic.complete() {
                continue;
            }
            if let Some(incoming) = receiver
                .topics
                .iter()
                .find(|entry| entry.topic == topic.topic)
                && incoming.processing == IncomingProcessing::Blocked
                && let Some(error) = incoming.error.as_ref()
            {
                topic.cause = Some(BarrierCause::Receiver(error.clone()));
            }
        }
        for status in &unavailable {
            let mut fresh = read_topic(context, &status.topic, Cursor::default());
            fresh.target = None;
            fresh.cause = status.cause.clone();
            topics.push(fresh);
        }
        if let Some(failure) = discovery_failure {
            if let Some(status) = topics
                .iter_mut()
                .find(|status| status.topic == failure.topic)
            {
                *status = failure;
            } else {
                topics.push(failure);
            }
        }
        if topics.iter().all(BarrierTopic::complete) {
            return Ok(BarrierSnapshot { topics });
        }
        let reason = if context.is_closed() {
            Some(BarrierFailure::Cancelled)
        } else if topics.iter().all(|topic| {
            topic.complete()
                || topic.blocked()
                    && (topic.target.is_none()
                        || receiver
                            .topics
                            .iter()
                            .any(|entry| entry.topic == topic.topic))
        }) {
            Some(BarrierFailure::Blocked)
        } else if Instant::now() >= deadline {
            Some(BarrierFailure::Deadline)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(BarrierError::Incomplete {
                reason,
                unfinished: topics
                    .into_iter()
                    .filter(|topic| !topic.complete())
                    .collect(),
            });
        }
        tokio::select! {
            _ = context.cancellation_token().cancelled() => {},
            _ = lease.changed() => {},
            _ = sleep(context.stream_settings().active_database_poll_interval.min(deadline.saturating_duration_since(Instant::now()))) => {},
        }
    }
}
