//! Re-add requests: who may re-add, sending outgoing requests, and validating and
//! answering incoming ones.

use super::*;

impl<Context> CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    /// Explicit request recipients; the one-shot sender also includes its own inbox.
    // implements: FORK-050
    async fn permitted_readders(&self, group_id: &GroupId) -> Result<Vec<String>, CommitLogError> {
        let (group, _) = MlsGroup::new_cached(self.context.clone(), group_id)?;
        Ok(group.with_group_snapshot(|g| {
            Ok::<_, GroupError>(ForkActors::from_group(g)?.request_targets(self.context.inbox_id()))
        })?)
    }

    async fn request_readd(
        &mut self,
        group_info: StoredGroupForReaddRequest,
    ) -> Result<(), CommitLogError> {
        let conn = self.context.db();
        let group_id = group_info.group_id;

        // Check if a readd request has already been sent for this group
        if conn.is_awaiting_readd(&group_id, self.context.installation_id().as_slice())? {
            tracing::debug!(
                group_id = %group_id,
                "Skipping readd request for group because it has already been requested"
            );
            return Ok(());
        }

        tracing::debug!(group_id = %group_id, "Sending readd request");

        // Send the one-shot request to the permitted readders.
        let latest_commit_sequence_id = group_info
            .latest_commit_sequence_id
            .ok_or(CommitLogError::MissingLatestCommitSequenceId { group_id })?;
        let oneshot_message = OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                group_id: group_id.to_vec(),
                latest_commit_sequence_id: latest_commit_sequence_id as u64,
            })),
        };
        let readders = self.permitted_readders(&group_id).await?;
        tracing::debug!(
            group_id = %group_id,
            "Sending readd request to {:?}",
            readders
        );
        // Mark readd as requested
        conn.update_requested_at_sequence_id(
            &group_id,
            self.context.installation_id().as_slice(),
            latest_commit_sequence_id,
        )?;

        if let Err((error, definitely_unsent)) =
            Oneshot::send_message_with_outcome(self.context.clone(), readders, oneshot_message)
                .await
        {
            if definitely_unsent {
                conn.delete_readd_statuses(
                    &group_id,
                    HashSet::from([self.context.installation_id().to_vec()]),
                )?;
            }
            return Err(error.into());
        }
        tracing::debug!(group_id = %group_id, "Sent readd request",);
        tracing::debug!(
            group_id = %group_id,
            sequence_id = latest_commit_sequence_id,
            "Updated requested readd sequence id",
        );

        Ok(())
    }

    /// Send readd requests for all forked conversations
    pub(super) async fn send_outgoing_readd_requests(&mut self) -> Result<(), CommitLogError> {
        if self.context.fork_recovery_opts().enable_recovery_requests == ForkRecoveryPolicy::None {
            return Ok(());
        }
        let conn = self.context.db();

        // Fetch all forked groups with their latest epoch
        let mut forked_groups = conn.get_conversation_ids_for_requesting_readds()?;
        if self.context.fork_recovery_opts().enable_recovery_requests
            == ForkRecoveryPolicy::AllowlistedGroups
        {
            let groups_to_request_recovery = self
                .context
                .fork_recovery_opts()
                .groups_to_request_recovery
                .iter()
                .map(|group_id| group_id.normalize_hex())
                .collect::<HashSet<String>>();
            tracing::info!(
                "Forked groups: {:?}, allowlisted groups for sending recovery requests: {:?}",
                forked_groups
                    .iter()
                    .map(|group_info| group_info.group_id.to_string())
                    .collect::<Vec<String>>(),
                groups_to_request_recovery
            );
            forked_groups.retain(|group_info| {
                groups_to_request_recovery
                    .contains(&group_info.group_id.to_string().normalize_hex())
            });
        }

        // Process groups in order, collecting per-group failures so one transient
        // error doesn't block the rest; a dropped pool still bubbles immediately.
        let mut failures = Vec::new();
        for group_info in forked_groups {
            let group_id = group_info.group_id;
            if let Err(source) = self.request_readd(group_info).await {
                if source.needs_db_reconnect() {
                    return Err(source);
                }
                failures.push(CommitLogError::FailedToSendReadd {
                    group_id,
                    source: Box::new(source),
                });
            }
        }

        if !failures.is_empty() {
            return Err(CommitLogError::FailedReadds { errors: failures });
        }

        Ok(())
    }

    pub(super) async fn handle_incoming_pending_readds(&mut self) -> Result<(), CommitLogError> {
        if self.context.fork_recovery_opts().disable_recovery_responses {
            return Ok(());
        }
        let now = xmtp_common::time::Instant::now();
        self.holds.lock().readd.retain(|_, until| now < *until);
        let conn = self.context.db();
        let groups_for_readd = conn.get_conversation_ids_for_responding_readds()?;

        // Runs every worker tick (~2s); only speak when there's actually work.
        if groups_for_readd.is_empty() {
            return Ok(());
        }
        tracing::debug!(
            "Processing readd requests for {} groups",
            groups_for_readd.len()
        );

        for group in groups_for_readd {
            // A rate-limited readd is not answered again at a later tick
            // inside its delay; other conversations still answer.
            // implements: RATE-041
            if self.holds.lock().readd.contains_key(&group.group_id) {
                continue;
            }
            match self.validate_pending_readds(&conn, &group).await {
                Ok(validated_installations) => {
                    if validated_installations.is_empty() {
                        continue;
                    }
                    let mls_group = MlsGroup::new(
                        self.context.clone(),
                        group.group_id,
                        group.dm_id.clone(),
                        group.conversation_type,
                        group.created_at_ns,
                    );
                    if let Err(e) = mls_group
                        .readd_installations(
                            validated_installations.into_iter().collect::<Vec<_>>(),
                        )
                        .await
                    {
                        let e = CommitLogError::from(e);
                        if e.needs_db_reconnect() {
                            return Err(e);
                        }
                        hold(
                            &mut self.holds.lock().readd,
                            group.group_id,
                            e.rate_limit_delay(),
                        );
                        tracing::warn!(
                            group_id = group.group_id.short_hex(),
                            "Keeping readd requests for group after a failed readd: {}",
                            e
                        );
                    }
                }
                Err(e) if e.needs_db_reconnect() => return Err(e),
                // The responder no longer consents to, is active in, or may
                // readd in the group: the requests are moot.
                Err(e @ CommitLogError::GroupReaddValidationError(_)) => {
                    tracing::warn!(
                        group_id = %group.group_id,
                        "Deleting readd statuses for group because it failed validation: {}",
                        e
                    );
                    conn.delete_other_readd_statuses(
                        &group.group_id,
                        self.context.installation_id().as_slice(),
                    )?;
                }
                // Reversible (e.g. a version pause): keep and retry next tick.
                Err(e) => {
                    hold(
                        &mut self.holds.lock().readd,
                        group.group_id,
                        e.rate_limit_delay(),
                    );
                    tracing::warn!(
                        group_id = group.group_id.short_hex(),
                        "Keeping readd requests for group after a failed validation: {}",
                        e
                    );
                }
            }
        }

        Ok(())
    }

    // implements: FORK-080, FORK-061
    async fn validate_pending_readds(
        &self,
        conn: &impl DbQuery,
        group: &StoredGroupForRespondingReadds,
    ) -> Result<HashSet<Vec<u8>>, CommitLogError> {
        let (mls_group, _) = MlsGroup::new_cached(self.context.clone(), &group.group_id)?;
        tracing::debug!(
            group_id = %mls_group.group_id,
            "Processing readd requests for group"
        );

        let synced = mls_group.sync_with_conn().await;
        // A sync never changes consent or activity, so these drop moot
        // requests even when it fails (e.g. a paused group).
        if mls_group.consent_state()? != ConsentState::Allowed {
            return Err(CommitLogError::GroupReaddValidationError(
                "Group is not consented".to_string(),
            ));
        }
        if !mls_group.is_active()? {
            return Err(CommitLogError::GroupReaddValidationError(
                "Group is not active".to_string(),
            ));
        }
        // An unsynced commit may have made this member a readder.
        synced?;
        let actors =
            mls_group.with_group_snapshot(|g| Ok::<_, GroupError>(ForkActors::from_group(g)?))?;
        if !actors.permits(self.context.inbox_id()) {
            return Err(CommitLogError::GroupReaddValidationError(
                "No longer a permitted readder of group".to_string(),
            ));
        }

        // A forked responder keeps the requests; a Welcome clears the fork.
        let fork_state = self.refresh_fork_state(&mls_group.group_id)?;
        if fork_state == Some(true) {
            tracing::info!(
                group_id = mls_group.group_id.short_hex(),
                "Responder is forked, keeping readd requests"
            );
            return Ok(HashSet::new());
        } else if fork_state.is_none() {
            tracing::info!(
                group_id = %mls_group.group_id,
                "Local commit log ahead of remote, skipping group"
            );
            return Ok(HashSet::new());
        }

        let readd_statuses = conn.get_readds_awaiting_response(
            &mls_group.group_id,
            self.context.installation_id().as_slice(),
        )?;
        let mut unverified = readd_statuses
            .iter()
            .map(|readd_status| readd_status.installation_id.clone())
            .collect::<HashSet<_>>();

        let (unverified, verified) = mls_group.with_group_snapshot(|openmls_group| {
            let mut verified = HashSet::new();
            for member in openmls_group.members() {
                if unverified.contains(&member.signature_key) {
                    unverified.remove(&member.signature_key);
                    verified.insert(member.signature_key);
                }
            }
            Ok::<_, GroupError>((unverified, verified))
        })?;
        tracing::debug!(
            group_id = %mls_group.group_id,
            "{} readd requests were for non-members, while {} were for members",
            unverified.len(),
            verified.len()
        );
        conn.delete_readd_statuses(&mls_group.group_id, unverified)?;

        let processed = conn
            .latest_cursor_for_id(
                mls_group.group_id,
                &[xmtp_db::refresh_state::EntityKind::ApplicationMessage],
            )?
            .0;
        let now = now_ns();
        // Unsigned wire claims above i64::MAX are stored as negative values.
        // Keep them pending instead of treating them as an earlier position.
        Ok(readd_statuses
            .into_iter()
            .filter(|status| {
                verified.contains(&status.installation_id)
                    && retry_due(&status.installation_id, now)
                    && status.requested_at_sequence_id.is_some_and(|requested| {
                        u64::try_from(requested).is_ok_and(|requested| requested <= processed)
                    })
            })
            .map(|status| status.installation_id)
            .collect())
    }
}
