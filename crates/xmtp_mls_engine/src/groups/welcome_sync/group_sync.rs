//! Group sync for the welcome service: syncing every group, unsticking paused groups,
//! and the combined welcome-and-group sync with its summary.

use super::*;

impl<Context> WelcomeService<Context>
where
    Context: XmtpSharedContext,
{
    /// Publish, process fixed targets, and finish required work under one deadline.
    /// Every selected group gets an outcome, even when another group fails.
    pub async fn sync_all_groups(
        &self,
        groups: Vec<MlsGroup<Context>>,
    ) -> Result<GroupSyncSummary, GroupError> {
        use crate::subscriptions::incoming::{IncomingCoordinator, IncomingScope};
        use xmtp_common::time::Instant;

        let deadline = Instant::now() + self.context.incoming_runtime().policy().barrier_timeout;
        let mut selected = HashSet::new();
        let selected: Vec<_> = groups
            .into_iter()
            .filter(|group| selected.insert(group.group_id))
            .collect();
        let db = self.context.db();
        let num_eligible = selected.len();
        let mut groups = Vec::new();
        let mut restored_count = 0;
        for group in selected {
            if db
                .find_group(&group.group_id)?
                .is_some_and(|stored| stored.membership_state == GroupMembershipState::Restored)
            {
                restored_count += 1;
            } else {
                groups.push(group);
            }
        }
        if groups.is_empty() && restored_count > 0 {
            return Err(GroupError::GroupInactive);
        }
        let topics = group_sync_topics(&groups);
        // Receipt can proceed while an independent outgoing request is pending.
        let _receipt = IncomingCoordinator::for_context(&self.context)
            .acquire(IncomingScope::Topics(topics.clone()));
        let concurrency = self
            .context
            .incoming_runtime()
            .policy()
            .max_dependency_requests;
        let mut summary = super::super::summary::SyncSummary::default();
        if restored_count > 0 {
            add_group_sync_error(&mut summary, GroupError::GroupInactive);
        }
        let publish =
            run_group_sync_work(&groups, GroupSyncWork::Publish, concurrency, deadline).await;
        for error in publish.into_iter().filter_map(Result::err) {
            summary.add_publish_err(error);
        }
        let received = crate::subscriptions::barrier::receive_through_current_until(
            &self.context,
            topics,
            deadline,
        )
        .await;
        let unfinished = unfinished_sync_topics(received.as_ref().err());
        if let Err(error) = received {
            add_group_sync_error(&mut summary, error.into());
        }
        let post_commit = run_group_sync_work(
            &groups,
            GroupSyncWork::PostCommit(&unfinished),
            concurrency,
            deadline,
        )
        .await;
        finish_group_sync(summary, num_eligible, post_commit)
    }

    /// Sweep every paused group and clear the pause flag for any
    /// whose `paused_for_version` is now satisfied by the client's
    /// `pkg_version`. Pure local-state operation — no network calls.
    ///
    /// Returns the count of groups unstuck. Safe to call on any
    /// installation regardless of whether any groups are paused
    /// (a no-op on installations with none).
    ///
    /// This is the recovery path for the "user upgrades but didn't
    /// touch a paused group" scenario: without this sweep a paused
    /// group could stay paused indefinitely after the upgrade, since
    /// `handle_group_paused` (which is the per-group re-evaluator)
    /// only fires when the group is actively synced — and
    /// `sync_all_welcomes_and_groups` filters out groups with no
    /// new messages on the server.
    pub async fn unstick_paused_groups(&self) -> Result<usize, GroupError> {
        use xmtp_mls_group_state::libxmtp_version::LibXMTPVersion;

        let paused = self.context.db().get_paused_groups_with_versions()?;
        if paused.is_empty() {
            return Ok(0);
        }
        // The client's own version is parsed once at `VersionInfo`
        // construction; reuse it across every paused group.
        let own_version_str = self.context.version_info().pkg_version().to_string();
        let own_v = self.context.version_info().pkg_semver();

        let mut unstuck = 0usize;
        for (group_id, required_str) in paused {
            // Lenient on malformed stored bytes — log and skip rather
            // than fail the whole sweep (one corrupted row shouldn't
            // brick recovery for all the others).
            let Ok(required_v) = LibXMTPVersion::parse(&required_str) else {
                tracing::warn!(
                    group_id = hex::encode(group_id.as_ref()),
                    required = %required_str,
                    "skipping unparseable paused_for_version while sweeping"
                );
                continue;
            };
            if required_v <= *own_v {
                // Same leniency as the parse-error branch above: a
                // transient DB failure on one row shouldn't abort the
                // sweep for the others. The next sync sweep will pick
                // this row up again.
                if let Err(err) = self.context.db().unpause_group(&group_id) {
                    tracing::warn!(
                        group_id = hex::encode(group_id.as_ref()),
                        required = %required_str,
                        error = %err,
                        "failed to unpause group during sweep; will retry on next sync"
                    );
                    continue;
                }
                tracing::debug!(
                    group_id = hex::encode(group_id.as_ref()),
                    required = %required_str,
                    own = %own_version_str,
                    "unstuck previously paused group: client version now satisfies floor"
                );
                unstuck += 1;
            }
        }
        Ok(unstuck)
    }

    /// Sync the initial groups and fixed-target Welcome discoveries under one deadline.
    /// Later local groups and above-target Welcomes do not expand this call's scope.
    pub async fn sync_all_welcomes_and_groups(
        &self,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<GroupSyncSummary, GroupError> {
        use crate::subscriptions::incoming::{IncomingCoordinator, IncomingScope};
        use xmtp_common::time::Instant;

        let deadline = Instant::now() + self.context.incoming_runtime().policy().barrier_timeout;
        // Fix the caller's initial group set before any asynchronous work.
        let groups: Vec<_> = self
            .context
            .db()
            .stored_group_refs(&GroupScope::listing(consent_states.clone()).without_restored())?
            .into_iter()
            .map(|group| self.mls_group(group))
            .collect();
        let initial_ids = groups.iter().map(|group| group.group_id).collect();
        let mut topics = group_sync_topics(&groups);
        topics.push(Topic::new_welcome_message(self.context.installation_id()));
        let _receipt =
            IncomingCoordinator::for_context(&self.context).acquire(IncomingScope::Topics(topics));
        let concurrency = self
            .context
            .incoming_runtime()
            .policy()
            .max_dependency_requests;
        let mut summary = super::super::summary::SyncSummary::default();
        if let Err(error) = self.unstick_paused_groups().await {
            add_group_sync_error(&mut summary, error);
        }
        let publish =
            run_group_sync_work(&groups, GroupSyncWork::Publish, concurrency, deadline).await;
        for error in publish.into_iter().filter_map(Result::err) {
            summary.add_publish_err(error);
        }
        let received = crate::subscriptions::barrier::receive_with_welcomes_until(
            &self.context,
            initial_ids,
            consent_states,
            deadline,
        )
        .await;
        let unfinished = unfinished_sync_topics(received.result.as_ref().err());
        if let Err(error) = received.result {
            add_group_sync_error(&mut summary, error.into());
        }
        let mut enrolled: std::collections::HashMap<_, _> = groups
            .into_iter()
            .map(|group| (group.group_id, group))
            .collect();
        let mut enrolled_ids: HashSet<_> = enrolled.keys().copied().collect();
        for group_id in received.group_ids {
            if !enrolled_ids.insert(group_id) {
                continue;
            }
            match MlsGroup::new_cached(self.context.clone(), &group_id) {
                Ok((group, _)) => {
                    enrolled.insert(group_id, group);
                }
                Err(error) => add_group_sync_error(&mut summary, error.into()),
            }
        }
        let num_eligible = enrolled_ids.len();
        let groups: Vec<_> = enrolled.into_values().collect();
        let post_commit = run_group_sync_work(
            &groups,
            GroupSyncWork::PostCommit(&unfinished),
            concurrency,
            deadline,
        )
        .await;
        finish_group_sync(summary, num_eligible, post_commit)
    }
}

#[derive(Clone, Copy)]
enum GroupSyncWork<'a> {
    Publish,
    PostCommit(&'a HashSet<Topic>),
}
fn unfinished_sync_topics(
    error: Option<&crate::subscriptions::barrier::BarrierError>,
) -> HashSet<Topic> {
    use crate::subscriptions::barrier::BarrierError;
    let Some(BarrierError::Incomplete { unfinished, .. }) = error else {
        return HashSet::new();
    };
    unfinished
        .iter()
        .map(|status| status.topic.clone())
        .collect()
}
fn group_sync_topics<Context: XmtpSharedContext>(groups: &[MlsGroup<Context>]) -> Vec<Topic> {
    groups
        .iter()
        .map(|group| Topic::new_group_message(group.group_id))
        .collect()
}

/// Every group gets an outcome, including work still queued when the deadline expires.
async fn run_group_sync_work<Context: XmtpSharedContext>(
    groups: &[MlsGroup<Context>],
    work: GroupSyncWork<'_>,
    concurrency: usize,
    deadline: xmtp_common::time::Instant,
) -> Vec<Result<bool, GroupError>> {
    use xmtp_common::time::{Instant, timeout};

    stream::iter(groups.iter().cloned())
        .map(|group| async move {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let timed_out = || GroupError::SyncFailedToWait(Box::default());
            if remaining.is_zero() {
                return Err(timed_out());
            }
            // A rate-limit wait past the deadline returns the rejection, so
            // its delay is reported instead of being lost to the timeout.
            let work = xmtp_common::retry::with_retry_deadline(deadline, async {
                match work {
                    GroupSyncWork::Publish => {
                        let active = group.is_active()?;
                        if active {
                            group.publish_intents().await?;
                        }
                        Ok(active)
                    }
                    GroupSyncWork::PostCommit(unfinished) => {
                        group.post_commit().await?;
                        if group.is_active()?
                            && !unfinished.contains(&Topic::new_group_message(group.group_id))
                        {
                            // Maintenance adds outgoing work, not a replacement fixed target.
                            // The outer timeout keeps this work within the same deadline.
                            group.maybe_update_installations(None).await?;
                        }
                        group.is_active()
                    }
                }
            });
            match timeout(remaining, work).await {
                Ok(result) => result,
                Err(_) => Err(timed_out()),
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await
}
fn add_group_sync_error(summary: &mut super::super::summary::SyncSummary, error: GroupError) {
    let error = match summary.other.take() {
        Some(first) => combine_sync_errors(*first, error),
        None => error,
    };
    summary.add_other(error);
}
fn finish_group_sync(
    mut summary: super::super::summary::SyncSummary,
    num_eligible: usize,
    post_commit: Vec<Result<bool, GroupError>>,
) -> Result<GroupSyncSummary, GroupError> {
    let mut num_synced = 0;
    for result in post_commit {
        match result {
            Ok(active) => num_synced += usize::from(active),
            Err(error) => summary.add_post_commit_err(error),
        }
    }
    if summary.is_errored() {
        Err(summary.into())
    } else {
        Ok(GroupSyncSummary::new(num_eligible, num_synced))
    }
}

fn combine_sync_errors(first: GroupError, second: GroupError) -> GroupError {
    use crate::subscriptions::barrier::{BarrierError, BarrierFailure};
    match (first, second) {
        (
            GroupError::StreamBarrier(BarrierError::Incomplete {
                reason: a,
                mut unfinished,
            }),
            GroupError::StreamBarrier(BarrierError::Incomplete {
                reason: b,
                unfinished: next,
            }),
        ) => {
            unfinished.extend(next);
            let reason = if a == BarrierFailure::Cancelled || b == BarrierFailure::Cancelled {
                BarrierFailure::Cancelled
            } else if a == BarrierFailure::Deadline || b == BarrierFailure::Deadline {
                BarrierFailure::Deadline
            } else {
                BarrierFailure::Blocked
            };
            BarrierError::Incomplete { reason, unfinished }.into()
        }
        (first, second) => {
            let mut summary = super::super::summary::SyncSummary::other(first);
            summary.add_publish_err(second);
            GroupError::from(summary)
        }
    }
}
