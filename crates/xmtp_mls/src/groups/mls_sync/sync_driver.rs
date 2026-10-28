//! Sync entry points and the intent-resolution loop.

use super::*;
use xmtp_common::snippet::Snippet;

fn log_publish_error(error: &GroupError) {
    if let GroupError::FailedToVerifyInstallations(failed) = error {
        tracing::error!(
            error_kind = "FailedToVerifyInstallations",
            failed_installation_count = failed.0.len(),
            "Sync: error publishing intents"
        );
    } else {
        tracing::error!("Sync: error publishing intents {error:?}");
    }
}

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    #[xmtp_common::mls_span]
    pub async fn sync(&self) -> Result<SyncSummary, GroupError> {
        self.sync_once()
            .await
            .map_err(|error| self.context.server_configuration().blocked_or(error))
    }

    /// Sync this group, then every other group of its stitched DM.
    ///
    /// The result is this group's own. Siblings sync even when this group
    /// fails, and a sibling's failure is logged, never returned, so one paused
    /// or failing group cannot stop the rest of the conversation. A paused
    /// sibling fails in `handle_group_paused` before it processes anything,
    /// so it stays held. If the siblings cannot be loaded, the call fails:
    /// with this group's error when it has one, else with the load error.
    // implements: DMS-019
    async fn sync_once(&self) -> Result<SyncSummary, GroupError> {
        // A client with a blocked connection makes no further calls.
        self.context.server_configuration().check()?;

        let conn = self.context.db();
        let result = async {
            if conn
                .find_group(&self.group_id)?
                .is_some_and(|group| group.membership_state == GroupMembershipState::Restored)
            {
                return Err(GroupError::GroupInactive);
            }
            let epoch = self.epoch().await?;
            tracing::debug!(
                inbox_id = self.context.inbox_id(),
                installation_id = self.context.installation_id().as_slice().snippet(),
                group_id = self.group_id.short_hex(),
                epoch,
                "syncing group",
            );
            let sync_summary = self.sync_with_conn().await.map_err(GroupError::from)?;
            self.maybe_update_installations(None).await?;
            Ok(sync_summary)
        }
        .await;

        let siblings = match conn.other_dms(&self.group_id) {
            Ok(siblings) => siblings,
            // The siblings were not synced, so the call does not succeed. The
            // named group's own failure still wins.
            Err(error) => {
                result?;
                return Err(error.into());
            }
        };
        for sibling in siblings {
            if sibling.membership_state == GroupMembershipState::Restored {
                continue;
            }
            let sibling = Self::new_from_arc(
                self.context.clone(),
                sibling.id,
                sibling.dm_id,
                sibling.conversation_type,
                sibling.created_at_ns,
            );
            if let Err(error) = sibling.sync_with_conn().await {
                tracing::warn!(
                    group_id = sibling.group_id.short_hex(),
                    ?error,
                    "stitched DM sibling sync failed"
                );
                continue;
            }
            if let Err(error) = sibling.maybe_update_installations(None).await {
                tracing::warn!(
                    group_id = sibling.group_id.short_hex(),
                    ?error,
                    "stitched DM sibling installation update failed"
                );
            }
        }

        result
    }

    fn handle_group_paused(&self) -> Result<(), GroupError> {
        // Check if group is paused and try to unpause if version requirements are met
        let group_id_typed = self.group_id;
        if let Some(required_min_version_str) = self
            .context
            .db()
            .get_group_paused_version(&group_id_typed)?
        {
            tracing::info!(
                "Group is paused until version: {}",
                required_min_version_str
            );
            let current_version_str = self.context.version_info().pkg_version();
            let current_version = self.context.version_info().pkg_semver();
            let required_min_version = LibXMTPVersion::parse(&required_min_version_str)
                .map_err(CommitValidationError::from)?;

            if required_min_version <= *current_version {
                tracing::info!(
                    "Unpausing group since version requirements are met. \
                     Group ID: {}",
                    hex::encode(self.group_id),
                );
                self.context.db().unpause_group(&group_id_typed)?;
            } else {
                tracing::warn!(
                    "Skipping sync for paused group since version requirements are not met. \
                    Group ID: {}, \
                    Required version: {}, \
                    Current version: {}",
                    hex::encode(self.group_id),
                    required_min_version_str,
                    current_version_str
                );
                // Skip sync for paused groups
                return Err(GroupError::GroupPausedUntilUpdate(required_min_version_str));
            }
        }
        Ok(())
    }

    /// Sync from the network with the 'conn' (local database).
    /// must return a summary of all messages synced, whether they were
    /// successful or not.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(err, fields(inbox_id = %self.context.inbox_id(), operation = "sync_with_conn")))]
    #[cfg_attr(not(any(test, feature = "test-utils")), xmtp_common::mls_span)]
    pub async fn sync_with_conn(&self) -> Result<SyncSummary, SyncSummary> {
        // App-data changes observed while processing are collected here and
        // dispatched below, *after* the per-group mutex is released. The block
        // exists to bound the guard's lifetime: a host callback is expected to
        // react by publishing its merged value, which re-enters
        // `sync_with_conn` and would deadlock on a guard still held here.
        let mut app_data_changes = Vec::new();
        let result = self.sync_with_conn_locked(&mut app_data_changes).await;
        self.dispatch_app_data_changes(app_data_changes).await;
        result
    }

    /// The body of [`Self::sync_with_conn`], holding the per-group mutex for
    /// its whole duration. Never dispatch host callbacks from in here.
    async fn sync_with_conn_locked(
        &self,
        app_data_changes: &mut Vec<AppDataChange>,
    ) -> Result<SyncSummary, SyncSummary> {
        let _mutex = self.mutex.lock().await;
        let mut summary = SyncSummary::default();

        if self
            .context
            .db()
            .find_group(&self.group_id)
            .map_err(|error| SyncSummary::other(error.into()))?
            .is_some_and(|group| group.membership_state == GroupMembershipState::Restored)
        {
            return Err(SyncSummary::other(GroupError::GroupInactive));
        }

        if !self.is_active().map_err(SyncSummary::other)? {
            log_event!(
                Event::GroupSyncGroupInactive,
                self.context.installation_id(),
                group_id = self.group_id
            );
            return Ok(summary);
        }

        if let Err(e) = self.handle_group_paused() {
            return Err(SyncSummary::other(e));
        }

        // Even if publish fails, continue to receiving
        let result = self.publish_intents().await;
        if let Err(e) = result {
            log_publish_error(&e);
            summary.add_publish_err(e);
        }

        // Even if receiving fails, we continue to post_commit
        // Errors are collected in the summary.
        let result = self.receive().await;
        match result {
            Ok(mut s) => {
                app_data_changes.append(&mut s.app_data_changes);
                summary.add_process(s)
            }
            Err(e) => {
                summary.add_other(e);
                // We don't return an error if receive fails, because it's possible this is caused
                // by malicious data sent over the network, or messages from before the user was
                // added to the group
            }
        }

        let result = self.post_commit().await;
        if let Err(e) = result {
            tracing::error!("post commit error {e:?}",);
            summary.add_post_commit_err(e);
        }

        if summary.is_errored() {
            Err(summary)
        } else {
            Ok(summary)
        }
    }

    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = %self.context.inbox_id()), skip_all))]
    #[cfg_attr(not(any(test, feature = "test-utils")), xmtp_common::mls_span)]
    pub(crate) async fn sync_until_last_intent_resolved(&self) -> Result<SyncSummary, GroupError> {
        // Filter to kinds this build understands: after a downgrade,
        // rows written by a newer build would otherwise fail `FromSql`
        // and poison the whole query (see `IntentKind::all`).
        let intents = self.context.db().find_group_intents(
            self.group_id,
            Some(vec![
                IntentState::ToPublish,
                IntentState::Published,
                IntentState::Committed,
            ]),
            Some(IntentKind::all().collect()),
        )?;

        let Some(intent) = intents.last() else {
            return Ok(Default::default());
        };

        self.sync_until_intent_resolved(intent.id).await
    }

    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(err, level = "info", fields(inbox_id = %self.context.inbox_id(), operation = "intent"), skip(self)))]
    #[cfg_attr(not(any(test, feature = "test-utils")), xmtp_common::mls_span)]
    /**
     * Sync the group and wait for the intent to be deleted
     * Group syncing may involve picking up messages unrelated to the intent, so simply checking for errors
     * does not give a clear signal as to whether the intent was successfully completed or not.
     *
     * Failed or stalled rounds use `xmtp_configuration::MAX_GROUP_SYNC_RETRIES`.
     * Completing an earlier state change does not consume that retry budget.
     * The same overall deadline bounds all rounds.
     */

    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    pub(crate) async fn sync_until_intent_resolved(
        &self,
        intent_id: ID,
    ) -> Result<SyncSummary, GroupError> {
        log_event!(
            Event::GroupSyncStart,
            self.context.installation_id(),
            group_id = self.group_id
        );

        // Keep the round future off the containing group sync poll stack.
        let result = async {
            let waited = async {
                let mut summary =
                    Box::pin(self.sync_until_intent_resolved_inner(intent_id)).await?;
                // A split target can become a no-op after another member completes
                // it. Its already-applied children can still owe Welcome delivery.
                for child in self.pending_membership_children(intent_id)? {
                    summary.extend(Box::pin(self.sync_until_intent_resolved_inner(child)).await?);
                }
                Ok::<_, GroupError>(summary)
            }
            .await;
            // implements: SEND-025
            // A part can end its batch in a round of a wait above, while the
            // original target is still publishing, and then no wait reads it.
            // That outcome wins over any error of another part's wait: the
            // membership change is applied, but its Welcomes were not sent.
            if self.membership_child_follow_up_failed(intent_id)? {
                return Err(super::publish::OutgoingPreparationError::WelcomesUnsendable.into());
            }
            waited
        }
        .await;
        let summary = match &result {
            Ok(summary) => Some(summary),
            Err(GroupError::Sync(summary)) => Some(&**summary),
            Err(GroupError::SyncFailedToWait(summary)) => Some(&**summary),
            _ => None,
        };

        log_event!(
            Event::GroupSyncFinished,
            self.context.installation_id(),
            group_id = self.group_id,
            summary = ?summary,
            success = result.is_ok()
        );

        result
    }

    async fn sync_until_intent_resolved_inner(
        &self,
        intent_id: ID,
    ) -> Result<SyncSummary, GroupError> {
        let mut summary = SyncSummary::default();
        let db = self.context.db();

        let time_spent = xmtp_common::time::Instant::now();
        let backoff = ExponentialBackoff::builder()
            .duration(Duration::from_millis(SYNC_BACKOFF_WAIT_MS.into()))
            .total_wait_max(Duration::from_secs(SYNC_BACKOFF_TOTAL_WAIT_MAX_SECS.into()))
            .max_jitter(Duration::from_millis(SYNC_JITTER_MS.into()))
            .build();

        // Return the last error to the caller if we fail to sync
        let mut attempt = 0;
        while attempt < MAX_GROUP_SYNC_RETRIES {
            let remaining = self
                .context
                .incoming_runtime()
                .policy()
                .barrier_timeout
                .saturating_sub(time_spent.elapsed());
            if remaining.is_zero() {
                break;
            }
            let predecessor = db
                .find_group_intents(
                    self.group_id,
                    Some(vec![
                        IntentState::ToPublish,
                        IntentState::Published,
                        IntentState::Committed,
                    ]),
                    Some(IntentKind::all().collect()),
                )?
                .into_iter()
                .find(|intent| intent.id < intent_id && intent.kind != IntentKind::SendMessage)
                .map(|intent| intent.id);
            let wait_for = backoff
                .backoff(attempt + 1, time_spent)
                .unwrap_or(Duration::from_millis(50));
            let membership_before = match Fetch::<StoredGroupIntent>::fetch(&db, &intent_id)? {
                Some(intent)
                    if intent.kind == IntentKind::UpdateGroupMembership
                        && intent.state == IntentState::ToPublish =>
                {
                    Some((
                        UpdateGroupMembershipIntentData::try_from(intent.data.as_slice())?,
                        self.with_group_snapshot(|group| {
                            extract_group_membership(group.extensions()).map_err(GroupError::from)
                        })?,
                    ))
                }
                _ => None,
            };

            log_event!(
                Event::GroupSyncAttempt,
                self.context.installation_id(),
                group_id = self.group_id,
                attempt,
                backoff = ?wait_for
            );

            // Accumulate each attempt's outcome into `summary`. The terminal
            // GroupSyncFinished event (in sync_until_intent_resolved) is the
            // single place the summary is logged — no per-attempt logging here.
            let mut round_succeeded = false;
            // A rate-limit wait inside the round ends by the round's timeout,
            // so the delay reaches this loop instead of being lost to it.
            let rate_limit_delay = match xmtp_common::time::timeout(
                remaining,
                xmtp_common::retry::with_retry_deadline(
                    xmtp_common::time::Instant::now() + remaining,
                    self.sync_intent_round(intent_id, predecessor, remaining),
                ),
            )
            .await
            {
                Ok(Ok(s)) => {
                    round_succeeded = !s.is_errored();
                    let delay = s.rate_limit_delay();
                    summary.extend(s);
                    delay
                }
                Ok(Err(error @ GroupError::PublishedButUnconfirmed { .. })) => return Err(error),
                Ok(Err(error)) => {
                    let delay = error.rate_limit_delay();
                    summary.add_other(error);
                    delay
                }
                Err(_) => break,
            };
            let current = Fetch::<StoredGroupIntent>::fetch(&db, &intent_id);
            let waiting_to_publish = matches!(
                &current,
                Ok(Some(intent)) if intent.state == IntentState::ToPublish
            );
            match current {
                Ok(Some(StoredGroupIntent {
                    state: IntentState::Processed,
                    ..
                })) => {
                    // This is expected, we mark intents as processed on success.
                    return Ok(summary);
                }
                Ok(None) => {
                    return Err(NotFound::IntentById(intent_id).into());
                }

                // Terminal: the guard no longer matched, so the intent will
                // never publish. Returning here rather than looping is what
                // keeps a superseded write from spinning until the sync
                // retry budget runs out. `update_app_data` inspects the state
                // and translates this into `AppDataSuperseded`.
                Ok(Some(StoredGroupIntent {
                    state: IntentState::Superseded,
                    kind,
                    ..
                })) => {
                    log_event!(
                        Event::GroupSyncIntentErrored,
                        self.context.installation_id(),
                        level = warn,
                        group_id = self.group_id, intent_id = intent_id,
                        intent_kind = ?kind
                    );
                    return Err(GroupError::from(summary));
                }

                Ok(Some(StoredGroupIntent {
                    state: IntentState::Error,
                    kind,
                    ..
                })) => {
                    // The summary itself is logged once by GroupSyncFinished;
                    // this event only marks which intent errored.
                    log_event!(
                        Event::GroupSyncIntentErrored,
                        self.context.installation_id(),
                        level = warn,
                        group_id = self.group_id, intent_id = intent_id,
                        intent_kind = ?kind
                    );
                    summary.extend(self.rejected_intent_summary(intent_id)?);
                    return Err(GroupError::from(summary));
                }
                // implements: SEND-025
                // The commit is applied, but its Welcomes were not sent, and
                // no later round can send them.
                Ok(Some(StoredGroupIntent {
                    state: IntentState::FollowUpFailed,
                    kind,
                    ..
                })) => {
                    log_event!(
                        Event::GroupSyncIntentErrored,
                        self.context.installation_id(),
                        level = warn,
                        group_id = self.group_id, intent_id = intent_id,
                        intent_kind = ?kind
                    );
                    return Err(super::publish::OutgoingPreparationError::WelcomesUnsendable.into());
                }
                Ok(Some(StoredGroupIntent { state, kind, .. })) => {
                    log_event!(
                        Event::GroupSyncIntentRetry,
                        self.context.installation_id(),
                        level = warn, group_id = self.group_id,
                        intent_id = intent_id, state = ?state, intent_kind = ?kind
                    );
                }
                Err(err) => {
                    tracing::error!(
                        group_id = %self.group_id,
                        intent_id,
                        attempt,
                        "database error fetching intent {err:?}"
                    );
                    summary.add_other(GroupError::Storage(err));
                }
            };
            // A round can finish a queued state change without publishing the
            // requested intent. That is progress, not a failed send attempt.
            if round_succeeded && waiting_to_publish {
                let predecessor_finished = if let Some(predecessor) = predecessor {
                    Fetch::<StoredGroupIntent>::fetch(&db, &predecessor)?
                        .is_some_and(|intent| intent.state == IntentState::Processed)
                } else {
                    false
                };
                // Split refresh children have later IDs. Count a round as
                // progress only when it completes a requested inbox change;
                // unrelated commits and repeated failures consume the budget.
                let membership_advanced = if let Some((target, before)) = membership_before {
                    let after = self.with_group_snapshot(|group| {
                        extract_group_membership(group.extensions()).map_err(GroupError::from)
                    })?;
                    target.membership_updates.iter().any(|(inbox, target)| {
                        before.get(inbox).is_none_or(|old| old < target)
                            && after.get(inbox).is_some_and(|new| new >= target)
                    }) || target
                        .removed_members
                        .iter()
                        .any(|inbox| before.get(inbox).is_some() && after.get(inbox).is_none())
                } else {
                    false
                };
                if predecessor_finished || membership_advanced {
                    continue;
                }
            }
            attempt += 1;
            if attempt < MAX_GROUP_SYNC_RETRIES {
                let remaining = self
                    .context
                    .incoming_runtime()
                    .policy()
                    .barrier_timeout
                    .saturating_sub(time_spent.elapsed());
                // A rate-limited round waits out its delay, or returns the
                // rejection when the delay does not end within this call.
                // implements: RATE-041
                if let Some(delay) = rate_limit_delay {
                    if delay >= remaining {
                        break;
                    }
                    xmtp_common::time::sleep(wait_for.max(delay)).await;
                    continue;
                }
                xmtp_common::time::sleep(wait_for.min(remaining)).await;
            }
        }
        // The wait can end before a round reads the intent's state.
        match Fetch::<StoredGroupIntent>::fetch(&db, &intent_id)?.map(|intent| intent.state) {
            Some(IntentState::Processed) => return Ok(summary),
            // implements: SEND-025
            Some(IntentState::FollowUpFailed) => {
                return Err(super::publish::OutgoingPreparationError::WelcomesUnsendable.into());
            }
            _ => {}
        }
        if self.published_request_target(intent_id)?.is_some() {
            return Err(GroupError::PublishedButUnconfirmed {
                intent_id,
                cause: None,
                rate_limit_delay: summary.rate_limit_delay(),
            });
        }
        Err(GroupError::SyncFailedToWait(Box::new(summary)))
    }

    /// Observe the current attempt at its accepted receipt target, not a later topic head.
    /// Before the attempt has a receipt, observe the receipt of `predecessor`, the state
    /// change that blocks it. A `QueryNewest` head can trail that receipt (OPS-009).
    async fn sync_intent_round(
        &self,
        intent_id: ID,
        predecessor: Option<ID>,
        timeout: Duration,
    ) -> Result<SyncSummary, GroupError> {
        use xmtp_proto::types::Topic;
        let started = xmtp_common::time::Instant::now();
        let mut summary = SyncSummary::default();
        match xmtp_common::time::timeout(timeout, self.publish_intents()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => summary.add_publish_err(error),
            Err(_) => return Err(GroupError::SyncFailedToWait(Box::new(summary))),
        }
        // A later error in this round must not hide a rate-limited publish's
        // delay, so the round ends with the summary that holds it.
        // implements: RATE-041
        let publish_delay = summary.rate_limit_delay();
        let receipt = self.request_read_target(intent_id)?;
        let blocker = match receipt {
            Some(_) => None,
            None => self.blocking_target(predecessor)?,
        };
        let topic = Topic::new_group_message(self.group_id);
        let targets = match receipt.or(blocker) {
            Some(target) => [(topic, target)].into(),
            None => match self.context.api().newest_topic_cursors(vec![topic]).await {
                Ok(targets) => targets,
                Err(error) if publish_delay.is_some() => {
                    summary.add_other(error.into());
                    return Ok(summary);
                }
                Err(error) => return Err(error.into()),
            },
        };
        if let Err(cause) = crate::subscriptions::barrier::wait_through(
            &self.context,
            targets,
            Some(timeout.saturating_sub(started.elapsed())),
        )
        .await
        {
            if self.published_request_target(intent_id)?.is_some() {
                return Err(GroupError::PublishedButUnconfirmed {
                    intent_id,
                    cause: Some(Box::new(cause)),
                    rate_limit_delay: publish_delay,
                });
            }
            if publish_delay.is_some() {
                summary.add_other(cause.into());
                return Ok(summary);
            }
            return Err(cause.into());
        }
        if let Err(error) = self.post_commit().await {
            summary.add_post_commit_err(error);
        }
        Ok(summary)
    }

    /// The receipt of the state change that holds an unpublished intent.
    /// Nothing is prepared while a state change is published, so a published
    /// one is the blocker, whatever its id: a commit of the pending proposals
    /// queued for a deferred message comes after it. Otherwise it is the
    /// predecessor, which can already be processed.
    // implements: SEND-022
    fn blocking_target(&self, predecessor: Option<ID>) -> Result<Option<Cursor>, GroupError> {
        let published = self
            .context
            .db()
            .find_group_intents(
                self.group_id,
                Some(vec![IntentState::Published]),
                Some(
                    IntentKind::all()
                        .filter(|kind| *kind != IntentKind::SendMessage)
                        .collect(),
                ),
            )?
            .into_iter()
            .map(|intent| intent.id)
            .min();
        match published.or(predecessor) {
            Some(blocker) => self.published_intent_target(blocker),
            None => Ok(None),
        }
    }

    pub(super) fn validate_message_epoch(
        inbox_id: InboxIdRef<'_>,
        intent_id: i32,
        group_epoch: GroupEpoch,
        message_epoch: GroupEpoch,
        max_past_epochs: usize,
    ) -> Result<(), GroupMessageProcessingError> {
        #[cfg(any(test, feature = "test-utils"))]
        crate::utils::test_mocks_helpers::maybe_mock_future_epoch_for_tests()?;

        if message_epoch.as_u64() + max_past_epochs as u64 <= group_epoch.as_u64() {
            tracing::warn!(
                inbox_id,
                message_epoch = message_epoch.as_u64(),
                group_epoch = group_epoch.as_u64(),
                intent_id,
                "[{}] message epoch {} is {} or more less than the group epoch {} for intent {}. Retrying message",
                inbox_id,
                message_epoch,
                max_past_epochs,
                group_epoch.as_u64(),
                intent_id
            );
            return Err(GroupMessageProcessingError::OldEpoch(
                message_epoch.as_u64(),
                group_epoch.as_u64(),
            ));
        } else if message_epoch.as_u64() > group_epoch.as_u64() {
            // Should not happen, logging proactively
            tracing::error!(
                inbox_id,
                message_epoch = message_epoch.as_u64(),
                group_epoch = group_epoch.as_u64(),
                intent_id,
                "[{}] message epoch {} is greater than group epoch {} for intent {}. Retrying message",
                inbox_id,
                message_epoch,
                group_epoch,
                intent_id
            );
            return Err(GroupMessageProcessingError::FutureEpoch(
                message_epoch.as_u64(),
                group_epoch.as_u64(),
            ));
        }
        Ok(())
    }
}
