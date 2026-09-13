//! Bounded catch-up through the shared receiver and fixed processing barriers.

use super::barrier::{BarrierError, receive_with_welcomes_until};
use crate::{Client, context::XmtpSharedContext, groups::GroupError};
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
}

impl<C: XmtpSharedContext + 'static> Client<C> {
    /// Process fixed starting targets and only the discoveries from the enrolled Welcomes.
    pub async fn catch_up_to_live(
        &self,
        timeout: Option<Duration>,
    ) -> Result<CatchUpSummary, CatchUpError> {
        let timeout = timeout.unwrap_or(self.context.incoming_runtime().policy().barrier_timeout);
        let started = Instant::now();
        let query = || GroupQueryArgs {
            include_sync_groups: true,
            include_duplicate_dms: true,
            ..Default::default()
        };
        let db = self.context.db();
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
            &self.context,
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
        let settings = self.context.incoming_runtime().policy();
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
}

#[cfg(test)]
mod tests {
    use super::CatchUpSummary;
    use crate::{tester, utils::MlsGroupExt};
    use xmtp_db::group_message::MsgQueryArgs;
    /// A client that has never streamed or synced catches up: the pending
    /// welcome joins the group (discovery adds an update on the same
    /// stream) and the group's history lands in the store.
    #[xmtp_common::test(unwrap_try = true)]
    async fn catch_up_joins_pending_groups_and_stores_history() {
        tester!(alix);
        tester!(bo);

        let group = bo.create_group(None, None)?;
        group.invite(&alix).await?;
        group.send_msg(b"while you were out").await;
        group.send_msg(b"still out").await;

        let summary = alix.catch_up_to_live(None).await?;

        let alix_group = alix.group(&group.group_id)?;
        let bodies: Vec<Vec<u8>> = alix_group
            .find_messages(&MsgQueryArgs::default())?
            .into_iter()
            .map(|m| m.decrypted_message_bytes)
            .collect();
        assert!(bodies.contains(&b"while you were out".to_vec()));
        assert!(bodies.contains(&b"still out".to_vec()));
        assert_eq!(summary.conversations, 1, "the joined group must be counted");
        assert!(
            summary.messages >= 2,
            "the stored history must be counted (got {})",
            summary.messages
        );
    }

    /// A member with a stale durable cursor gets the missed tail replayed
    /// and stored — and a second run finds nothing new to do.
    #[xmtp_common::test(unwrap_try = true)]
    async fn catch_up_replays_the_missed_tail_idempotently() {
        tester!(alix);
        tester!(bo);

        let group = alix.create_group(None, None)?;
        group.invite(&bo).await?;
        bo.sync_welcomes().await?;
        let bo_group = bo.group(&group.group_id)?;
        bo_group.sync().await?; // durable cursor at "now"

        group.send_msg(b"missed one").await;
        group.send_msg(b"missed two").await;

        let first = bo.catch_up_to_live(None).await?;
        let count_after_first = bo_group.find_messages(&MsgQueryArgs::default())?.len();
        let bodies: Vec<Vec<u8>> = bo_group
            .find_messages(&MsgQueryArgs::default())?
            .into_iter()
            .map(|m| m.decrypted_message_bytes)
            .collect();
        assert!(bodies.contains(&b"missed one".to_vec()));
        assert!(bodies.contains(&b"missed two".to_vec()));
        assert!(
            first.messages >= 2,
            "the replayed tail must be counted (got {})",
            first.messages
        );
        assert_eq!(first.conversations, 0, "no new group was joined");

        // The honesty proof for the counter: the replay of already-stored
        // history persists nothing, so the summary must say so.
        let second = bo.catch_up_to_live(None).await?;
        let count_after_second = bo_group.find_messages(&MsgQueryArgs::default())?.len();
        assert_eq!(count_after_first, count_after_second);
        assert_eq!(
            second,
            CatchUpSummary {
                completed: true,
                ..Default::default()
            },
            "a second run still reaches live, but persists nothing, so its counts are zero"
        );
    }

    /// Nothing owed at all — a fresh client's run is just the welcome-topic
    /// update acknowledgement and the empty target set.
    #[xmtp_common::test(unwrap_try = true)]
    async fn catch_up_with_nothing_owed_completes() {
        tester!(alix);
        let summary = alix.catch_up_to_live(None).await?;
        assert_eq!(
            summary,
            CatchUpSummary {
                completed: true,
                ..Default::default()
            },
            "nothing owed still completes, with zero counts"
        );
    }
}
