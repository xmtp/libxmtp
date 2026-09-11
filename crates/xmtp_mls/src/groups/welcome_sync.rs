use crate::context::XmtpSharedContext;
use crate::groups::InitialMembershipValidator;
#[cfg(test)]
use crate::groups::ValidateGroupMembership;
use crate::groups::XmtpWelcome;
use crate::groups::mls_ext::ResolvedWelcome;
use crate::groups::{GroupError, MlsGroup};
use crate::identity_updates::{
    IdentityDependencyError, IdentityRequirement, InstallationDiffError,
    resolve_identity_requirement,
};
#[cfg(test)]
use crate::intents::ProcessIntentError;
#[cfg(test)]
use crate::mls_store::MlsStore;
use futures::stream::{self, StreamExt};
use prost::Message;
use std::collections::HashSet;
#[cfg(test)]
use xmtp_common::Event;
use xmtp_common::RetryableError;
use xmtp_db::incoming_envelope::{
    IncomingRetry, NetworkEntityKind, StoredIncomingEnvelope, StreamTopic,
};
#[cfg(test)]
use xmtp_db::refresh_state::EntityKind;
use xmtp_db::{consent_record::ConsentState, group::GroupQueryArgs, prelude::*};
#[cfg(test)]
use xmtp_macro::log_event;
use xmtp_proto::types::Topic;
use xmtp_proto::types::{Cursor, GroupId};

const WELCOME_POINTER_RETENTION_NS: i64 = 3 * xmtp_common::NS_IN_DAY;

/// Work needed before one durable Welcome can be attempted again.
pub(crate) enum WelcomeRequirement {
    /// The exact inbox and sequence proof required by the staged membership.
    Identity(IdentityRequirement),
    /// Pointer data that must be fetched without a database writer.
    Pointee,
    /// An active older group must process its ordered prefix before replacement.
    GroupPrefix {
        /// Existing group whose active state prevents the join.
        group_id: GroupId,
        /// Last group log position already covered by the joined state.
        anchor: Cursor,
    },
}

/// Local attempt result. A waiting Welcome does not stop independent joins.
pub(crate) enum WelcomeHeadOutcome<C> {
    /// The pending row was already completed by another attempt.
    Idle { cursor: Cursor },
    /// The row remains durable with a retry or blocked diagnostic.
    Waiting {
        cursor: Cursor,
        code: String,
        /// Blocked rows are retried once per coordinator, not by a timer.
        blocked: bool,
    },
    /// The scheduler must resolve a dependency outside the state writer.
    Need {
        cursor: Cursor,
        requirement: WelcomeRequirement,
    },
    /// The join or safe rejection and pending-row completion have committed.
    Progress {
        cursor: Cursor,
        result: Result<Option<MlsGroup<C>>, GroupError>,
    },
}

fn unsupported_welcome_wire(wire: &xmtp_proto::backend_v1::ServerEnvelope) -> bool {
    use xmtp_proto::backend_v1::{client_envelope::Payload, welcome_message::Version};
    use xmtp_proto::xmtp::mls::message_contents::{
        WelcomePointerWrapperAlgorithm, WelcomeWrapperAlgorithm,
    };
    match wire
        .envelope
        .as_ref()
        .and_then(|envelope| envelope.payload.as_ref())
    {
        Some(Payload::WelcomeMessage(welcome)) => match &welcome.version {
            None => true,
            Some(Version::V1(message)) => {
                WelcomeWrapperAlgorithm::try_from(message.wrapper_algorithm).is_err()
            }
            Some(Version::WelcomePointer(message)) => {
                WelcomePointerWrapperAlgorithm::try_from(message.wrapper_algorithm).is_err()
            }
        },
        _ => false,
    }
}

#[derive(Debug, Clone)]
/// Counts returned only after all selected sync phases succeed.
pub struct GroupSyncSummary {
    /// Distinct selected groups, including groups found through the fixed Welcome target.
    pub num_eligible: usize,
    /// Selected groups that remain active after required outgoing work completes.
    pub num_synced: usize,
}

impl GroupSyncSummary {
    pub fn new(num_eligible: usize, num_synced: usize) -> Self {
        Self {
            num_eligible,
            num_synced,
        }
    }
}

// Outcome of span-instrumented welcome processing: the expected
// already-processed duplicate is an `Ok` variant so it cannot mark the
// `mls.process_new_welcome` span as status:error.
#[cfg(test)]
enum WelcomeOutcome<Context> {
    Processed(Option<MlsGroup<Context>>),
    AlreadyProcessed(Cursor),
}

#[derive(Clone)]
/// Welcome processing and group sync through the context's shared incoming coordinator.
pub struct WelcomeService<Context> {
    context: Context,
}

impl<Context> WelcomeService<Context> {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

impl<Context> WelcomeService<Context>
where
    Context: XmtpSharedContext,
{
    /// Admit a test Welcome, then atomically join or record a safe rejection.
    // Callers still receive `Err(WelcomeAlreadyProcessed)` for the routine
    // duplicate-delivery case, but the span lives on the inner fn where that
    // expected outcome exits as `Ok` — so it never flags span status:error.
    #[cfg(test)]
    pub(crate) async fn process_new_welcome(
        &self,
        welcome: &xmtp_proto::types::WelcomeMessage,
        validator: impl ValidateGroupMembership,
    ) -> Result<Option<MlsGroup<Context>>, GroupError> {
        match self.process_new_welcome_spanned(welcome, validator).await? {
            WelcomeOutcome::Processed(group) => Ok(group),
            WelcomeOutcome::AlreadyProcessed(cursor) => Err(GroupError::ProcessIntent(
                ProcessIntentError::WelcomeAlreadyProcessed(cursor),
            )),
        }
    }

    #[cfg(test)]
    #[tracing::instrument(err, skip_all, fields(operation = "mls.process_new_welcome"))]
    async fn process_new_welcome_spanned(
        &self,
        welcome: &xmtp_proto::types::WelcomeMessage,
        validator: impl ValidateGroupMembership,
    ) -> Result<WelcomeOutcome<Context>, GroupError> {
        let pending = self
            .context
            .db()
            .pending_envelope(&self.topic(), welcome.cursor)?
            .ok_or(ProcessIntentError::WelcomeAlreadyProcessed(welcome.cursor))?;
        let result = XmtpWelcome::builder()
            .context(self.context.clone())
            .welcome(welcome)
            .pending(pending)
            .validator(validator)
            .process()
            .await;

        match result {
            Ok(mls_group) => {
                if let Some(mls_group) = &mls_group {
                    if let (Ok(epoch), Ok(auth)) = (
                        mls_group.epoch().await,
                        mls_group.epoch_authenticator().await,
                    ) {
                        log_event!(
                            Event::ReceivedWelcome,
                            self.context.installation_id(),
                            group_id = mls_group.group_id.as_slice(),
                            conversation_type = %mls_group.conversation_type,
                            epoch,
                            epoch_auth = auth
                        );
                    } else {
                        tracing::warn!(
                            "Failed to lock the mls group for logging ProcessedWelcome."
                        );
                    }
                }

                Ok(WelcomeOutcome::Processed(mls_group))
            }
            Err(err) => {
                use crate::DuplicateItem::*;
                use crate::StorageError::*;

                if matches!(err, GroupError::Storage(Duplicate(WelcomeId(_)))) {
                    tracing::warn!(
                        welcome_id = %welcome.cursor,
                        "Welcome ID already stored: {}",
                        err
                    );
                    return Ok(WelcomeOutcome::AlreadyProcessed(welcome.cursor));
                } else if let GroupError::ProcessIntent(
                    ProcessIntentError::WelcomeAlreadyProcessed(cursor),
                ) = err
                {
                    // Expected, non-retryable condition: the welcome was already
                    // processed (e.g. duplicate delivery for a group we are already
                    // in). It is handled gracefully upstream (cursor incremented,
                    // welcome skipped), so log at warn rather than error to avoid
                    // marking the span as status:error and inflating the error rate.
                    tracing::warn!(
                        welcome_id = %welcome.cursor,
                        "welcome already processed, skipping: {}",
                        err
                    );
                    return Ok(WelcomeOutcome::AlreadyProcessed(cursor));
                } else {
                    tracing::error!(
                        "failed to create group from welcome={} created at {}: {}",
                        welcome.cursor,
                        welcome.created_ns.timestamp(),
                        err
                    );
                }

                Err(err)
            }
        }
    }

    fn topic(&self) -> StreamTopic {
        StreamTopic {
            entity_id: self.context.installation_id().to_vec(),
            kind: NetworkEntityKind::Welcome,
        }
    }

    /// Attempt a bounded set of independent Welcomes. This method does no network I/O.
    pub(crate) fn process_pending_welcomes_once(
        &self,
    ) -> Result<Vec<WelcomeHeadOutcome<Context>>, GroupError> {
        let settings = self.context.stream_settings();
        self.context
            .db()
            .ready_welcomes_bounded(
                xmtp_common::time::now_ns(),
                settings.max_fetched_rows,
                settings.max_fetched_bytes,
            )?
            .into_iter()
            .filter(|pending| pending.entity_id == self.context.installation_id())
            .map(|pending| self.attempt_pending_welcome(pending, None, None))
            .collect()
    }

    /// Reread one durable row after a dependency completes.
    /// Only the exact resolver result can mark an identity reference as absent.
    pub(crate) fn retry_pending_welcome(
        &self,
        cursor: Cursor,
        missing_reference: Option<&IdentityRequirement>,
    ) -> Result<WelcomeHeadOutcome<Context>, GroupError> {
        let Some(pending) = self.context.db().pending_envelope(&self.topic(), cursor)? else {
            return Ok(WelcomeHeadOutcome::Idle { cursor });
        };
        self.attempt_pending_welcome(pending, None, missing_reference)
    }

    /// Recheck blocked Welcomes once when a coordinator starts.
    /// Advance `after` to each returned cursor until a batch is empty.
    /// Complete this scan before processing ready Welcomes in that coordinator.
    pub(crate) fn retry_blocked_welcomes_after(
        &self,
        after: Cursor,
    ) -> Result<Vec<WelcomeHeadOutcome<Context>>, GroupError> {
        let settings = self.context.stream_settings();
        self.context
            .db()
            .blocked_welcomes_bounded(
                &self.topic(),
                after,
                settings.max_fetched_rows,
                settings.max_fetched_bytes,
            )?
            .into_iter()
            .map(|pending| self.attempt_pending_welcome(pending, None, None))
            .collect()
    }

    fn attempt_pending_welcome(
        &self,
        pending: StoredIncomingEnvelope,
        resolved: Option<&ResolvedWelcome>,
        missing_reference: Option<&IdentityRequirement>,
    ) -> Result<WelcomeHeadOutcome<Context>, GroupError> {
        let cursor = Cursor(pending.sequence_id as u64);
        let wire = xmtp_proto::backend_v1::ServerEnvelope::decode(pending.envelope.as_slice())
            .map_err(|_| xmtp_db::StorageError::DbDeserialize)?;
        if unsupported_welcome_wire(&wire) {
            self.defer_pending(&pending, "unsupported_welcome", true, None)?;
            return Ok(WelcomeHeadOutcome::Waiting {
                cursor,
                code: "unsupported_welcome".into(),
                blocked: true,
            });
        }
        let welcome = match xmtp_api_backend::envelope::decode_welcome_message(wire) {
            Ok(welcome) => welcome,
            Err(error) => {
                let error = GroupError::WrappedApi(xmtp_api::ApiError::Envelope(error));
                self.complete_rejected_pending(&pending, "invalid_welcome_envelope")?;
                return Ok(WelcomeHeadOutcome::Progress {
                    cursor,
                    result: Err(error),
                });
            }
        };
        if let Some(deadline) = pending.retry_expires_at_ns
            && deadline <= xmtp_common::time::now_ns()
        {
            self.complete_rejected_pending(&pending, "welcome_pointer_expired")?;
            return Ok(WelcomeHeadOutcome::Progress {
                cursor,
                result: Err(GroupError::WelcomeDataNotFound("expired pointer".into())),
            });
        }
        let inline = ResolvedWelcome::inline(&welcome);
        let Some(resolved) = resolved.or(inline.as_ref()) else {
            self.defer_pending(
                &pending,
                "welcome_pointee",
                false,
                Some(
                    welcome
                        .timestamp()
                        .saturating_add(WELCOME_POINTER_RETENTION_NS),
                ),
            )?;
            return Ok(WelcomeHeadOutcome::Need {
                cursor,
                requirement: WelcomeRequirement::Pointee,
            });
        };
        let result = XmtpWelcome::builder()
            .context(self.context.clone())
            .welcome(&welcome)
            .pending(pending.clone())
            .validator(InitialMembershipValidator::new(&self.context))
            .process_resolved(resolved, missing_reference);
        self.classify_pending_result(&pending, result)
    }

    fn classify_pending_result(
        &self,
        pending: &StoredIncomingEnvelope,
        result: Result<Option<MlsGroup<Context>>, GroupError>,
    ) -> Result<WelcomeHeadOutcome<Context>, GroupError> {
        let cursor = Cursor(pending.sequence_id as u64);
        match result {
            Ok(group) => Ok(WelcomeHeadOutcome::Progress {
                cursor,
                result: Ok(group),
            }),
            Err(GroupError::InstallationDiff(InstallationDiffError::IdentityDependency(
                IdentityDependencyError::Need(requirement),
            ))) => {
                self.defer_pending(pending, "identity_dependency", false, None)?;
                Ok(WelcomeHeadOutcome::Need {
                    cursor,
                    requirement: WelcomeRequirement::Identity(requirement),
                })
            }
            Err(GroupError::WelcomeGroupPrefixPending { group_id, anchor }) => {
                self.defer_pending(pending, "group_prefix", false, None)?;
                Ok(WelcomeHeadOutcome::Need {
                    cursor,
                    requirement: WelcomeRequirement::GroupPrefix {
                        group_id,
                        anchor: Cursor(anchor),
                    },
                })
            }
            Err(error) => {
                if crate::groups::welcomes::terminal_welcome_error(&error) {
                    self.complete_rejected_pending(pending, "invalid_welcome")?;
                }
                if self
                    .context
                    .db()
                    .pending_envelope(&self.topic(), cursor)?
                    .is_none()
                {
                    return Ok(WelcomeHeadOutcome::Progress {
                        cursor,
                        result: Err(error),
                    });
                }
                let blocked = !error.is_retryable()
                    || matches!(
                        error,
                        GroupError::UnsupportedWelcomeVersion(_)
                            | GroupError::WelcomeError(
                                openmls::prelude::WelcomeError::UnsupportedMlsVersion
                                    | openmls::prelude::WelcomeError::UnsupportedExtensions
                                    | openmls::prelude::WelcomeError::UnsupportedCapability
                                    | openmls::prelude::WelcomeError::UnsupportedCiphersuite(_)
                            )
                    );
                let code = if blocked {
                    "welcome_blocked"
                } else {
                    "welcome_retry"
                };
                self.defer_pending(pending, code, blocked, None)?;
                tracing::warn!(sequence_id = cursor.0, code, error = %error, "Welcome remains pending");
                Ok(WelcomeHeadOutcome::Waiting {
                    cursor,
                    code: code.into(),
                    blocked,
                })
            }
        }
    }

    /// Keep retry state without extending a pointer's original expiry deadline.
    fn defer_pending(
        &self,
        pending: &StoredIncomingEnvelope,
        code: &'static str,
        blocked: bool,
        deadline: Option<i64>,
    ) -> Result<(), GroupError> {
        let delay = self.context.stream_settings().active_database_poll_interval;
        self.context.db().set_incoming_retry(
            &self.topic(),
            Cursor(pending.sequence_id as u64),
            &IncomingRetry {
                retry_at_ns: xmtp_common::time::now_ns().saturating_add(delay.as_nanos() as i64),
                blocked,
                error_code: Some(code.into()),
                retry_expires_at_ns: deadline,
            },
        )?;
        Ok(())
    }

    /// Commit rejection progress only if the exact pending bytes still match.
    fn complete_rejected_pending(
        &self,
        pending: &StoredIncomingEnvelope,
        code: &'static str,
    ) -> Result<(), GroupError> {
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let cursor = Cursor(pending.sequence_id as u64);
            if db
                .pending_envelope(&self.topic(), cursor)?
                .is_some_and(|current| current.envelope == pending.envelope)
            {
                db.record_terminal_rejection(&self.topic(), cursor, code)?;
                db.complete_pending_envelope(&self.topic(), cursor)?;
            }
            Ok::<_, GroupError>(xmtp_db::TransactionOutcome::Continue(()))
        })?;
        Ok(())
    }

    /// Resolve one Welcome's dependencies. Group-prefix work stays with the coordinator.
    pub(crate) async fn resolve_pending_welcome(
        &self,
        cursor: Cursor,
    ) -> Result<WelcomeHeadOutcome<Context>, GroupError> {
        let Some(pending) = self.context.db().pending_envelope(&self.topic(), cursor)? else {
            return Ok(WelcomeHeadOutcome::Idle { cursor });
        };
        if pending
            .retry_expires_at_ns
            .is_some_and(|deadline| deadline <= xmtp_common::time::now_ns())
        {
            return self.attempt_pending_welcome(pending, None, None);
        }
        let wire = xmtp_proto::backend_v1::ServerEnvelope::decode(pending.envelope.as_slice())
            .map_err(|_| xmtp_db::StorageError::DbDeserialize)?;
        let welcome = xmtp_api_backend::envelope::decode_welcome_message(wire)
            .map_err(xmtp_api::ApiError::from)?;
        let resolved = match ResolvedWelcome::resolve(&welcome, &self.context).await {
            Ok(resolved) => resolved,
            Err(GroupError::WelcomeDataNotFound(_)) => {
                self.defer_pending(
                    &pending,
                    "welcome_pointee",
                    false,
                    Some(
                        welcome
                            .timestamp()
                            .saturating_add(WELCOME_POINTER_RETENTION_NS),
                    ),
                )?;
                return Ok(WelcomeHeadOutcome::Waiting {
                    cursor,
                    code: "welcome_pointee".into(),
                    blocked: false,
                });
            }
            Err(error) => return self.classify_pending_result(&pending, Err(error)),
        };
        loop {
            let outcome = self.attempt_pending_welcome(pending.clone(), Some(&resolved), None)?;
            let WelcomeHeadOutcome::Need {
                requirement: WelcomeRequirement::Identity(requirement),
                ..
            } = outcome
            else {
                return Ok(outcome);
            };
            match resolve_identity_requirement(&self.context, &requirement).await {
                Ok(()) => {}
                Err(IdentityDependencyError::MissingReference(_)) => {
                    return self.attempt_pending_welcome(
                        pending,
                        Some(&resolved),
                        Some(&requirement),
                    );
                }
                Err(error) => {
                    return self.classify_pending_result(
                        &pending,
                        Err(InstallationDiffError::from(error).into()),
                    );
                }
            }
        }
    }

    /// Process all Welcomes through one fixed replica-visible target.
    pub async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Context>>, GroupError> {
        let args = GroupQueryArgs {
            include_sync_groups: true,
            include_duplicate_dms: true,
            ..Default::default()
        };
        let before: HashSet<_> = self
            .context
            .db()
            .fetch_conversation_list(args.clone())?
            .into_iter()
            .map(|group| group.id)
            .collect();
        crate::subscriptions::barrier::receive_through_current(
            &self.context,
            vec![Topic::new_welcome_message(self.context.installation_id())],
        )
        .await?;
        Ok(self
            .context
            .db()
            .fetch_conversation_list(args)?
            .into_iter()
            .filter(|group| !before.contains(&group.id))
            .map(|group| {
                MlsGroup::new(
                    self.context.clone(),
                    group.id,
                    group.dm_id,
                    group.conversation_type,
                    group.created_at_ns,
                )
            })
            .collect())
    }

    /// Publish, process fixed targets, and finish required work under one deadline.
    /// Every selected group gets an outcome, even when another group fails.
    pub async fn sync_all_groups(
        &self,
        groups: Vec<MlsGroup<Context>>,
    ) -> Result<GroupSyncSummary, GroupError> {
        use crate::subscriptions::incoming::{IncomingCoordinator, IncomingScope};
        use xmtp_common::time::Instant;

        let deadline = Instant::now() + self.context.stream_settings().barrier_timeout;
        let mut selected = HashSet::new();
        let groups: Vec<_> = groups
            .into_iter()
            .filter(|group| selected.insert(group.group_id))
            .collect();
        let num_eligible = groups.len();
        let topics = group_sync_topics(&groups);
        // Receipt can proceed while an independent outgoing request is pending.
        let _receipt = IncomingCoordinator::for_context(&self.context)
            .acquire(IncomingScope::Topics(topics.clone()));
        let concurrency = self.context.stream_settings().max_dependency_requests;
        let mut summary = super::summary::SyncSummary::default();
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
        use crate::groups::validated_commit::LibXMTPVersion;

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

        let deadline = Instant::now() + self.context.stream_settings().barrier_timeout;
        // Fix the caller's initial group set before any asynchronous work.
        let groups: Vec<_> = self
            .context
            .db()
            .fetch_conversation_list(GroupQueryArgs {
                consent_states: consent_states.clone(),
                include_duplicate_dms: true,
                include_sync_groups: true,
                ..Default::default()
            })?
            .into_iter()
            .map(|group| {
                MlsGroup::new(
                    self.context.clone(),
                    group.id,
                    group.dm_id,
                    group.conversation_type,
                    group.created_at_ns,
                )
            })
            .collect();
        let initial_ids = groups.iter().map(|group| group.group_id).collect();
        let mut topics = group_sync_topics(&groups);
        topics.push(Topic::new_welcome_message(self.context.installation_id()));
        let _receipt =
            IncomingCoordinator::for_context(&self.context).acquire(IncomingScope::Topics(topics));
        let concurrency = self.context.stream_settings().max_dependency_requests;
        let mut summary = super::summary::SyncSummary::default();
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
            match timeout(remaining, async {
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
            })
            .await
            {
                Ok(result) => result,
                Err(_) => Err(timed_out()),
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await
}

fn add_group_sync_error(summary: &mut super::summary::SyncSummary, error: GroupError) {
    let error = match summary.other.take() {
        Some(first) => combine_sync_errors(*first, error),
        None => error,
    };
    summary.add_other(error);
}

fn finish_group_sync(
    mut summary: super::summary::SyncSummary,
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
            let mut summary = super::summary::SyncSummary::other(first);
            summary.add_publish_err(second);
            GroupError::from(summary)
        }
    }
}

#[cfg(test)]
pub(crate) async fn pending_welcome_for_test(
    context: &impl XmtpSharedContext,
    welcome: &xmtp_proto::types::WelcomeMessage,
) -> Result<StoredIncomingEnvelope, GroupError> {
    let topic = StreamTopic {
        entity_id: context.installation_id().to_vec(),
        kind: NetworkEntityKind::Welcome,
    };
    let store = MlsStore::new(context.clone());
    loop {
        if let Some(pending) = context.db().pending_envelope(&topic, welcome.cursor)? {
            return Ok(pending);
        }
        let page = store
            .receive_topics_once(
                &[Topic::new_welcome_message(context.installation_id())],
                context
                    .stream_settings()
                    .incoming_limits(NetworkEntityKind::Welcome),
            )
            .await?;
        if !page.has_more {
            return context
                .db()
                .pending_envelope(&topic, welcome.cursor)?
                .ok_or_else(|| ProcessIntentError::WelcomeAlreadyProcessed(welcome.cursor).into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups::WelcomeMembership;
    use crate::tester;
    use crate::utils::test::MlsGroupExt;
    use rstest::*;

    struct RejectMembership {
        retryable: bool,
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn blocked_welcome_does_not_hold_an_independent_join() {
        use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
        use xmtp_db::ConnectionExt;
        use xmtp_db::schema::incoming_envelopes;
        use xmtp_proto::backend_v1::{client_envelope::Payload, welcome_message::Version};

        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let first_group = alix.create_group(None, None)?;
        first_group.invite(&bo).await?;
        let second_group = alix.create_group(None, None)?;
        second_group.invite(&bo).await?;
        let welcomes = bo
            .context
            .api()
            .query_welcome_messages(bo.context.installation_id())
            .await?;
        let first = welcomes.first()?.cursor;
        let last = welcomes.last()?.cursor;
        pending_welcome_for_test(&bo.context, welcomes.last()?).await?;
        let service = WelcomeService::new(bo.context.clone());
        let db = bo.context.db();
        let pending = db.pending_envelope(&service.topic(), first)?.unwrap();
        let mut wire = xmtp_proto::backend_v1::ServerEnvelope::decode(pending.envelope.as_slice())?;
        let Payload::WelcomeMessage(welcome) = wire.envelope.as_mut()?.payload.as_mut()? else {
            panic!("expected Welcome payload");
        };
        match welcome.version.as_mut()? {
            Version::V1(message) => message.wrapper_algorithm = i32::MAX,
            Version::WelcomePointer(message) => message.wrapper_algorithm = i32::MAX,
        }
        db.raw_query(|conn| {
            diesel::update(incoming_envelopes::table.find((
                bo.context.installation_id().to_vec(),
                EntityKind::Welcome,
                first.0 as i64,
            )))
            .set(incoming_envelopes::envelope.eq(wire.encode_to_vec()))
            .execute(conn)
        })?;

        let outcomes = service.process_pending_welcomes_once()?;
        assert!(outcomes.iter().any(|outcome| matches!(outcome,
            WelcomeHeadOutcome::Waiting { cursor, blocked: true, .. } if *cursor == first)));
        assert!(outcomes.iter().any(|outcome| matches!(outcome,
            WelcomeHeadOutcome::Need { cursor, .. } | WelcomeHeadOutcome::Progress { cursor, .. } if *cursor == last)));
        if db.find_group(&second_group.group_id)?.is_none() {
            assert!(matches!(
                service.resolve_pending_welcome(last).await?,
                WelcomeHeadOutcome::Progress {
                    result: Ok(Some(_)),
                    ..
                }
            ));
        }
        assert!(db.find_group(&first_group.group_id)?.is_none());
        assert!(db.find_group(&second_group.group_id)?.is_some());
        let first_pending = db.pending_envelope(&service.topic(), first)?.unwrap();
        assert!(first_pending.blocked);
        assert_eq!(
            first_pending.error_code.as_deref(),
            Some("unsupported_welcome")
        );
        assert!(db.pending_envelope(&service.topic(), last)?.is_none());
        let progress = db.topic_progress(&service.topic())?;
        assert!(progress.processed < first);
        assert_eq!(progress.received, last);
    }

    impl ValidateGroupMembership for RejectMembership {
        async fn check_initial_membership(
            &self,
            _welcome: &WelcomeMembership,
        ) -> Result<(), GroupError> {
            if self.retryable {
                Err(GroupError::LockUnavailable)
            } else {
                Err(GroupError::NoPSKSupport)
            }
        }
    }

    #[rstest]
    #[case::terminal(false)]
    #[case::retry(true)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn welcome_rejection_commits_only_terminal_progress(#[case] retryable: bool) {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let group = alix.create_group(None, None).unwrap();
        group.invite(&bo).await.unwrap();
        let welcome = bo
            .context
            .api()
            .query_welcome_messages(bo.context.installation_id())
            .await
            .unwrap()
            .pop()
            .unwrap();
        pending_welcome_for_test(&bo.context, &welcome)
            .await
            .unwrap();
        let service = WelcomeService::new(bo.context.clone());
        let result = service
            .process_new_welcome(&welcome, RejectMembership { retryable })
            .await;
        assert!(result.is_err());
        assert!(
            bo.context
                .db()
                .find_group(&group.group_id)
                .unwrap()
                .is_none()
        );
        let expected = if !retryable {
            welcome.cursor
        } else {
            Cursor(0)
        };
        assert_eq!(
            bo.context
                .db()
                .get_last_cursor(bo.context.installation_id(), EntityKind::Welcome)
                .unwrap(),
            expected
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn later_welcome_completion_keeps_earlier_retry_pending() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let first_group = alix.create_group(None, None)?;
        first_group.invite(&bo).await?;
        let second_group = alix.create_group(None, None)?;
        second_group.invite(&bo).await?;
        let welcomes = bo
            .context
            .api()
            .query_welcome_messages(bo.context.installation_id())
            .await?;
        let first = welcomes.first()?.cursor;
        let last = welcomes.last()?.cursor;
        pending_welcome_for_test(&bo.context, welcomes.last()?).await?;
        let service = WelcomeService::new(bo.context.clone());
        for welcome in welcomes {
            let result = service
                .process_new_welcome(
                    &welcome,
                    RejectMembership {
                        retryable: welcome.cursor == first,
                    },
                )
                .await;
            assert!(result.is_err());
        }
        let db = bo.context.db();
        assert!(db.pending_envelope(&service.topic(), first)?.is_some());
        assert!(db.pending_envelope(&service.topic(), last)?.is_none());
        let progress = db.topic_progress(&service.topic())?;
        assert!(progress.processed < first);
        assert_eq!(progress.received, last);
    }
}
