//! Bounded catch-up through the shared receiver and fixed processing barriers.

use std::collections::{HashMap, HashSet};
use xmtp_common::{
    ErrorCode, RetryableError,
    time::{Duration, Instant, now_ns},
};
use xmtp_db::{
    delivery::{DeliveryScope, QueryDelivery},
    group::{ConversationType, GroupQueryArgs},
    incoming_envelope::{QueryIncomingEnvelope, StreamTopic},
    prelude::*,
};
use xmtp_mls_engine::groups::{GroupError, context::XmtpSharedContext};
use xmtp_mls_engine::subscriptions::barrier::{BarrierError, receive_with_welcomes_until};

/// Committed catch-up observations; fixed processing predicates determine completion.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CatchUpSummary {
    /// Newly deliverable retained rows observed in enrolled groups during the run.
    pub messages: u64,
    /// Newly discovered non-virtual conversations within the enrolled scope.
    pub conversations: u64,
    /// Enrolled groups with a newly recorded terminal rejection.
    pub failed: u64,
    /// True only when every fixed processing obligation completed.
    pub completed: bool,
}

#[derive(Debug, thiserror::Error, ErrorCode)]
pub enum CatchUpError {
    /// Target discovery or storage failed. May be retryable.
    #[error(transparent)]
    #[error_code(inherit)]
    Group(#[from] GroupError),
    /// Fixed targets remain unfinished. Partial committed progress is retained. May be retryable.
    #[error("Catch-up did not complete: {summary:?}")]
    Incomplete {
        /// Partial progress remains committed even when the barrier fails.
        summary: CatchUpSummary,
        /// Every unfinished fixed-target obligation and its typed cause.
        causes: Vec<BarrierError>,
    },
}

impl RetryableError for CatchUpError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Group(error) => error.is_retryable(),
            Self::Incomplete { causes, .. } => causes.iter().any(RetryableError::is_retryable),
        }
    }

    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        match self {
            Self::Group(error) => error.rate_limit_delay(),
            Self::Incomplete { causes, .. } => causes
                .iter()
                .filter_map(RetryableError::rate_limit_delay)
                .max(),
        }
    }
}

impl From<xmtp_mls_server_config::server_configuration::BlockedConnection> for CatchUpError {
    fn from(error: xmtp_mls_server_config::server_configuration::BlockedConnection) -> Self {
        Self::Group(error.into())
    }
}

/// Process fixed starting targets and only the discoveries from the enrolled Welcomes.
pub async fn catch_up_to_live<C: XmtpSharedContext + 'static>(
    context: &C,
    timeout: Option<Duration>,
) -> Result<CatchUpSummary, CatchUpError> {
    catch_up_once(context, timeout)
        .await
        .map_err(|error| context.server_configuration().blocked_or(error))
}

async fn catch_up_once<C: XmtpSharedContext + 'static>(
    context: &C,
    timeout: Option<Duration>,
) -> Result<CatchUpSummary, CatchUpError> {
    // A client with a blocked connection sends no topic or credential to the backend.
    context
        .server_configuration()
        .check()
        .map_err(GroupError::from)?;
    let timeout = timeout.unwrap_or(context.stream_policy().barrier_timeout);
    let started = Instant::now();
    let query = || GroupQueryArgs {
        include_sync_groups: true,
        include_duplicate_dms: true,
        ..Default::default()
    };
    let db = context.db();
    let mut position = db.current_delivery_cursor().map_err(GroupError::from)?;
    let before = db.find_groups(query()).map_err(GroupError::from)?;
    let before_ids: HashSet<_> = before.iter().map(|group| group.id).collect();
    let previous_rejections: HashMap<_, _> = before
        .iter()
        .map(|group| {
            Ok((
                group.id,
                db.read_last_rejection(&StreamTopic::group(group.id))?
                    .map(|entry| entry.sequence_id)
                    .unwrap_or_default(),
            ))
        })
        .collect::<Result<_, xmtp_db::StorageError>>()
        .map_err(GroupError::from)?;
    let run = receive_with_welcomes_until(
        context,
        before_ids.iter().copied().collect(),
        None,
        started + timeout,
    )
    .await;
    let causes: Vec<_> = run.result.err().into_iter().collect();
    let enrolled: HashSet<_> = run.group_ids.iter().copied().collect();
    let groups: Vec<_> = db
        .find_groups(query())
        .map_err(GroupError::from)?
        .into_iter()
        .filter(|group| enrolled.contains(&group.id))
        .collect();
    let upper = db.current_delivery_cursor().map_err(GroupError::from)?;
    let settings = context.stream_policy();
    let mut summary = CatchUpSummary {
        conversations: groups
            .iter()
            .filter(|group| {
                !before_ids.contains(&group.id)
                    && !ConversationType::virtual_types().contains(&group.conversation_type)
            })
            .count() as u64,
        completed: causes.is_empty(),
        ..Default::default()
    };
    for group in &groups {
        if let Some(rejection) = db
            .read_last_rejection(&StreamTopic::group(group.id))
            .map_err(GroupError::from)?
            && rejection.sequence_id
                > previous_rejections
                    .get(&group.id)
                    .copied()
                    .unwrap_or_default()
        {
            summary.failed += 1;
        }
    }
    loop {
        let rows = db
            .replay_delivery_messages_bounded(
                position,
                &DeliveryScope::Groups(run.group_ids.clone()),
                now_ns(),
                settings.max_local_read_rows,
                settings.max_local_read_bytes,
            )
            .map_err(GroupError::from)?;
        if rows.is_empty() {
            break;
        }
        let mut reached_upper = false;
        for row in rows {
            if row.cursor.delivery_sequence > upper.delivery_sequence {
                reached_upper = true;
                break;
            }
            position = row.cursor;
            summary.messages += 1;
        }
        if reached_upper || position.delivery_sequence >= upper.delivery_sequence {
            break;
        }
    }
    if !causes.is_empty() {
        return Err(CatchUpError::Incomplete { summary, causes });
    }
    Ok(summary)
}
