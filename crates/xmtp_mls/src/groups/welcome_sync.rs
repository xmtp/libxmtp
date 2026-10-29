//! Durable Welcome processing for the incoming coordinator and client sync APIs.
//!
//! `WelcomeService` installs independent joins, resolves their dependencies, and
//! holds Welcomes with unsupported formats, unavailable keys, unresolved pointers,
//! or unreached group prefixes for a bounded window.
//! `WelcomeHeadOutcome` tells the coordinator which work remains; `GroupSyncSummary`
//! reports completed group synchronization.

use crate::context::XmtpSharedContext;
use crate::groups::InitialMembershipValidator;
#[cfg(test)]
use crate::groups::ValidateGroupMembership;
use crate::groups::XmtpWelcome;
use crate::groups::mls_ext::ResolvedWelcome;
use crate::groups::{GroupError, MlsGroup};
use crate::identity_updates::{
    IdentityDependencyError, IdentityRequirement, InstallationDiffError,
    resolve_identity_requirements,
};
#[cfg(test)]
use crate::intents::ProcessIntentError;
#[cfg(test)]
use crate::mls_store::MlsStore;
use futures::stream::{self, StreamExt};
use prost::Message;
use std::collections::{BTreeSet, HashSet};
#[cfg(test)]
use xmtp_common::Event;
use xmtp_common::RetryableError;
use xmtp_configuration::WELCOME_RETENTION_NS;
use xmtp_db::incoming_envelope::{
    IncomingRetry, NetworkEntityKind, StoredIncomingEnvelope, StreamTopic,
};
#[cfg(test)]
use xmtp_db::refresh_state::EntityKind;
use xmtp_db::{
    consent_record::ConsentState,
    group::{GroupMembershipState, GroupScope, StoredGroupRef},
    prelude::*,
};
#[cfg(test)]
use xmtp_macro::log_event;
use xmtp_proto::types::Topic;
use xmtp_proto::types::{Cursor, GroupId};

/// How long a Welcome this build cannot process is retained before it is
/// completed and removed. A client that supports it within the window still
/// joins; past the window the row must not keep the Welcome barrier, the
/// Welcome budget, and key-package retirement blocked forever.
const UNSUPPORTED_WELCOME_RETENTION_NS: i64 = xmtp_common::NS_IN_DAY;

/// Work needed before one durable Welcome can be attempted again.
pub(crate) enum WelcomeRequirement {
    /// Every exact inbox and sequence proof the staged membership lacks.
    Identity(BTreeSet<IdentityRequirement>),
    /// Pointer data that must be fetched without a database writer.
    Pointee,
    /// An active older group must process its ordered prefix before replacement.
    GroupPrefix {
        /// Existing group whose active state prevents the join.
        group_id: GroupId,
        /// Last group log position already covered by the joined state.
        anchor: Cursor,
        /// Stored retry deadline. At or after it the coordinator reads the
        /// group topic's newest position before it may expire the hold.
        deadline_ns: i64,
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
        /// Blocked rows are retried at coordinator startup and on periodic scans.
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

/// The backend's timestamp on a stored Welcome, the base of its retention deadlines.
fn welcome_timestamp_ns(pending: &StoredIncomingEnvelope) -> Result<i64, GroupError> {
    let wire = xmtp_proto::backend_v1::ServerEnvelope::decode(pending.envelope.as_slice())
        .map_err(|_| xmtp_db::StorageError::DbDeserialize)?;
    let welcome = xmtp_api_backend::envelope::decode_welcome_message(wire)
        .map_err(xmtp_api::ApiError::from)?;
    Ok(welcome.timestamp())
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

    /// Attempt independent Welcomes excluding those already owned by dependency work.
    /// Apply row/byte budgets after exclusions. This method does no network I/O.
    pub(crate) fn process_pending_welcomes_once(
        &self,
        excluded: &[Cursor],
    ) -> Result<Vec<WelcomeHeadOutcome<Context>>, GroupError> {
        let settings = self.context.incoming_runtime().policy();
        self.context
            .db()
            .ready_welcomes_bounded(
                xmtp_common::time::now_ns(),
                settings.max_fetched_rows,
                settings.max_fetched_bytes,
                excluded,
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

    /// Recheck blocked Welcomes at startup and on periodic coordinator scans.
    /// Advance `after` to each returned cursor until a batch is empty.
    /// Complete this scan before processing ready Welcomes in that coordinator.
    pub(crate) fn retry_blocked_welcomes_after(
        &self,
        after: Cursor,
    ) -> Result<Vec<WelcomeHeadOutcome<Context>>, GroupError> {
        let settings = self.context.incoming_runtime().policy();
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
            return self.retain_unreadable_welcome(
                &pending,
                GroupError::UnsupportedWelcomeVersion("unsupported wrapper algorithm".into()),
                "unsupported_welcome",
            );
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
        // Unreadable Welcomes get a real attempt even after their deadline: an
        // upgrade or recovered key may now allow them to join.
        if let Some(deadline) = pending.retry_expires_at_ns
            && deadline <= xmtp_common::time::now_ns()
            && pending.error_code.as_deref() == Some("welcome_pointee")
            && pending.unreadable_expires_at_ns.is_none()
        {
            self.complete_rejected_pending(&pending, "welcome_pointer_expired")?;
            return Ok(WelcomeHeadOutcome::Progress {
                cursor,
                result: Err(GroupError::WelcomeDataNotFound("expired pointer".into())),
            });
        }
        let inline = ResolvedWelcome::inline(&welcome);
        let Some(resolved) = resolved.or(inline.as_ref()) else {
            // Only a failed fetch establishes a pointee hold. Overwriting an
            // unreadable hold here would skip its final resolution attempt.
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

    // implements: JOIN-084, JOIN-085
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
                IdentityDependencyError::Need(requirements),
            ))) => {
                self.defer_pending(pending, "identity_dependency", false, None)?;
                Ok(WelcomeHeadOutcome::Need {
                    cursor,
                    requirement: WelcomeRequirement::Identity(requirements),
                })
            }
            // implements: JOIN-092
            // A deadline alone never rejects: only the coordinator's final
            // catch-up can, so a restart past the deadline reads the group
            // topic before it decides.
            Err(GroupError::WelcomeGroupPrefixPending { group_id, anchor }) => {
                let deadline_ns = match pending.retry_expires_at_ns {
                    Some(deadline) => deadline,
                    None => welcome_timestamp_ns(pending)?.saturating_add(WELCOME_RETENTION_NS),
                };
                self.defer_pending(pending, "group_prefix", false, Some(deadline_ns))?;
                Ok(WelcomeHeadOutcome::Need {
                    cursor,
                    requirement: WelcomeRequirement::GroupPrefix {
                        group_id,
                        anchor: Cursor(anchor),
                        deadline_ns,
                    },
                })
            }
            Err(
                error @ (GroupError::WelcomeError(
                    openmls::prelude::WelcomeError::UnsupportedMlsVersion
                    | openmls::prelude::WelcomeError::UnsupportedExtensions
                    | openmls::prelude::WelcomeError::UnsupportedCapability
                    | openmls::prelude::WelcomeError::UnsupportedCiphersuite(_),
                )
                | GroupError::NotFound(
                    xmtp_db::NotFound::KeyPackageReference(_)
                    | xmtp_db::NotFound::KeyPackage(_)
                    | xmtp_db::NotFound::PostQuantumPrivateKey,
                )),
            ) => self.retain_unreadable_welcome(pending, error, "welcome_blocked"),
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
                    || matches!(error, GroupError::UnsupportedWelcomeVersion(_));
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

    /// Bound unreadable input only after attempting it with the current code and keys.
    // implements: JOIN-087, JOIN-063
    fn retain_unreadable_welcome(
        &self,
        pending: &StoredIncomingEnvelope,
        error: GroupError,
        code: &'static str,
    ) -> Result<WelcomeHeadOutcome<Context>, GroupError> {
        let cursor = Cursor(pending.sequence_id as u64);
        let now = xmtp_common::time::now_ns();
        let Some(deadline) = self.context.db().set_unreadable_welcome_deadline(
            self.context.installation_id().as_slice(),
            cursor,
            now.saturating_add(UNSUPPORTED_WELCOME_RETENTION_NS),
        )?
        else {
            return Ok(WelcomeHeadOutcome::Idle { cursor });
        };
        if deadline <= now {
            self.complete_rejected_pending(pending, "unsupported_welcome_expired")?;
            return Ok(WelcomeHeadOutcome::Progress {
                cursor,
                result: Err(error),
            });
        }
        self.defer_pending(pending, code, true, None)?;
        Ok(WelcomeHeadOutcome::Waiting {
            cursor,
            code: code.into(),
            blocked: true,
        })
    }

    /// Keep retry state without extending the original expiry deadline.
    fn defer_pending(
        &self,
        pending: &StoredIncomingEnvelope,
        code: &'static str,
        blocked: bool,
        deadline: Option<i64>,
    ) -> Result<(), GroupError> {
        let policy = self.context.incoming_runtime().policy();
        let delay = policy.active_database_poll_interval;
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
        if blocked || deadline.is_some() || pending.unreadable_expires_at_ns.is_some() {
            // implements: PROC-020
            // A full held allowance gives up the oldest held Welcome.
            let allowance = policy.incoming_limits(NetworkEntityKind::Welcome).kind;
            while let Some(oldest) = self
                .context
                .db()
                .oldest_held_welcome_over(&self.topic(), allowance)?
            {
                self.complete_rejected_pending(&oldest, "held_capacity")?;
            }
        }
        #[cfg(any(test, feature = "test-utils"))]
        crate::diagnostics::record_welcome_retry(code);
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

    /// Reject a group-prefix hold whose deadline has passed, given `target`, the
    /// newest position of the group's message topic read at or after that
    /// deadline. It rejects only when processing has reached `target` and stops
    /// below `anchor`, and the group's MLS state is still active. Otherwise the
    /// row stays pending for a later attempt.
    // implements: JOIN-092
    pub(crate) fn expire_group_prefix_welcome(
        &self,
        cursor: Cursor,
        group_id: GroupId,
        anchor: Cursor,
        target: Cursor,
    ) -> Result<WelcomeHeadOutcome<Context>, GroupError> {
        enum Expiry {
            Gone,
            Kept { code: String, blocked: bool },
            Rejected,
        }
        let now = xmtp_common::time::now_ns();
        let expiry = crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let Some(pending) = db.pending_envelope(&self.topic(), cursor)? else {
                return Ok::<_, GroupError>(xmtp_db::TransactionOutcome::Continue(Expiry::Gone));
            };
            let processed = db.topic_progress(&StreamTopic::group(group_id))?.processed;
            let expired = pending.error_code.as_deref() == Some("group_prefix")
                && pending
                    .retry_expires_at_ns
                    .is_some_and(|deadline| deadline <= now)
                && target <= processed
                && processed < anchor
                // Another process may already have processed the removal. An
                // inactive group lets the re-add replace it, so keep the Welcome.
                && openmls::group::MlsGroup::load(&storage, &group_id.to_openmls())?
                    .is_some_and(|group| group.is_active());
            if !expired {
                return Ok(xmtp_db::TransactionOutcome::Continue(Expiry::Kept {
                    code: pending.error_code.unwrap_or_default(),
                    blocked: pending.blocked,
                }));
            }
            db.record_terminal_rejection(&self.topic(), cursor, "group_prefix_expired")?;
            db.complete_pending_envelope(&self.topic(), cursor)?;
            Ok(xmtp_db::TransactionOutcome::Continue(Expiry::Rejected))
        })?
        .into_continued();
        Ok(match expiry {
            Expiry::Gone => WelcomeHeadOutcome::Idle { cursor },
            Expiry::Kept { code, blocked } => WelcomeHeadOutcome::Waiting {
                cursor,
                code,
                blocked,
            },
            Expiry::Rejected => WelcomeHeadOutcome::Progress {
                cursor,
                result: Err(GroupError::WelcomeGroupPrefixPending {
                    group_id,
                    anchor: anchor.0,
                }),
            },
        })
    }

    /// Resolve one Welcome's dependencies. Group-prefix work stays with the coordinator.
    // implements: JOIN-094
    pub(crate) async fn resolve_pending_welcome(
        &self,
        cursor: Cursor,
    ) -> Result<WelcomeHeadOutcome<Context>, GroupError> {
        let Some(pending) = self.context.db().pending_envelope(&self.topic(), cursor)? else {
            return Ok(WelcomeHeadOutcome::Idle { cursor });
        };
        if pending.error_code.as_deref() == Some("welcome_pointee")
            && pending.unreadable_expires_at_ns.is_none()
            && pending
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
            // API errors here come from querying the pointee, before any MLS or
            // identity processing. Failed queries obey the same retention as
            // missing data, regardless of whether the API error is retryable.
            Err(error @ (GroupError::WelcomeDataNotFound(_) | GroupError::WrappedApi(_))) => {
                // Once unreadable retention starts, failed re-fetches must
                // neither shorten that window nor extend it indefinitely.
                if pending.unreadable_expires_at_ns.is_some() {
                    return self.retain_unreadable_welcome(&pending, error, "welcome_blocked");
                }
                let deadline = pending
                    .retry_expires_at_ns
                    .unwrap_or_else(|| welcome.timestamp().saturating_add(WELCOME_RETENTION_NS));
                // A legacy blocked row gets a real fetch before its cutoff is
                // applied. Missing data still obeys the fixed pointer deadline.
                if deadline <= xmtp_common::time::now_ns() {
                    self.complete_rejected_pending(&pending, "welcome_pointer_expired")?;
                    return Ok(WelcomeHeadOutcome::Progress {
                        cursor,
                        result: Err(error),
                    });
                }
                // Preserve the legacy classification so its next attempt can
                // fetch and reclassify the pointee, even past the cutoff.
                let blocked = pending.error_code.as_deref() == Some("welcome_blocked")
                    || matches!(&error, GroupError::WrappedApi(api) if !api.is_retryable());
                let code = if blocked {
                    "welcome_blocked"
                } else {
                    "welcome_pointee"
                };
                self.defer_pending(&pending, code, blocked, Some(deadline))?;
                return Ok(WelcomeHeadOutcome::Waiting {
                    cursor,
                    code: code.into(),
                    blocked,
                });
            }
            Err(error) => return self.classify_pending_result(&pending, Err(error)),
        };
        loop {
            let outcome = self.attempt_pending_welcome(pending.clone(), Some(&resolved), None)?;
            let WelcomeHeadOutcome::Need {
                requirement: WelcomeRequirement::Identity(requirements),
                ..
            } = outcome
            else {
                return Ok(outcome);
            };
            match resolve_identity_requirements(&self.context, &requirements).await {
                Ok(()) => {}
                Err(IdentityDependencyError::MissingReference(requirement)) => {
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
        let scope = GroupScope::listing(None);
        let before: HashSet<_> = self
            .context
            .db()
            .stored_group_refs(&scope)?
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
            .stored_group_refs(&scope)?
            .into_iter()
            .filter(|group| !before.contains(&group.id))
            .map(|group| self.mls_group(group))
            .collect())
    }

    fn mls_group(&self, group: StoredGroupRef) -> MlsGroup<Context> {
        MlsGroup::new(
            self.context.clone(),
            group.id,
            group.dm_id,
            group.conversation_type,
            group.created_at_ns,
        )
    }

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
        let mut summary = super::summary::SyncSummary::default();
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
        use xmtp_mls_common::libxmtp_version::LibXMTPVersion;

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
                    .incoming_runtime()
                    .policy()
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

/// Alix's group, from which Alix removed and then re-added Bo. Bo still holds
/// the group as active: it received the group topic only through its removal,
/// because the secrets of its old membership cannot process the re-add commit,
/// and it processed none of it. The re-add Welcome is pending.
#[cfg(test)]
pub(crate) struct HeldReadd {
    pub(crate) alix_group: crate::utils::TestMlsGroup,
    pub(crate) bo_group: crate::utils::TestMlsGroup,
    pub(crate) welcome: xmtp_proto::types::WelcomeMessage,
    /// Bo's receipt of the group topic through its removal.
    pub(crate) removal: xmtp_db::incoming_envelope::AdmissionResult,
}

#[cfg(test)]
pub(crate) async fn held_readd(
    alix: &crate::utils::FullXmtpClient,
    bo: &crate::utils::FullXmtpClient,
) -> HeldReadd {
    let alix_group = alix.create_group(None, None).unwrap();
    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();
    let bo_group = bo.sync_welcomes().await.unwrap().pop().unwrap();
    alix_group.remove_members(&[bo.inbox_id()]).await.unwrap();
    let page = MlsStore::new(bo.context.clone())
        .receive_topics_once(
            &[Topic::new_group_message(bo_group.group_id)],
            bo.context
                .incoming_runtime()
                .policy()
                .incoming_limits(NetworkEntityKind::Group),
        )
        .await
        .unwrap();
    assert!(!page.has_more);
    let removal = page.admissions[0].1;
    assert!(removal.inserted > 0);
    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();
    let welcome = bo
        .context
        .api()
        .query_welcome_messages(bo.context.installation_id())
        .await
        .unwrap()
        .into_iter()
        .max_by_key(|welcome| welcome.cursor)
        .unwrap();
    pending_welcome_for_test(&bo.context, &welcome)
        .await
        .unwrap();
    HeldReadd {
        alix_group,
        bo_group,
        welcome,
        removal,
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

    #[derive(Clone, Copy)]
    enum UnreadableWelcome {
        Wrapper,
        MlsVersion,
        MissingKey,
    }

    /// Unreadable input must expire without extending its deadline, including
    /// legacy blocked rows. A held Welcome does not stop admission of the next
    /// one; expiry releases the Welcome barrier and key retirement.
    // verifies: JOIN-087, JOIN-084, JOIN-063
    #[rstest]
    #[case::wrapper(UnreadableWelcome::Wrapper)]
    #[case::mls_version(UnreadableWelcome::MlsVersion)]
    #[case::missing_key(UnreadableWelcome::MissingKey)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn an_unsupported_welcome_expires_after_a_final_attempt(
        #[case] failure: UnreadableWelcome,
        #[values(false, true)] previously_blocked: bool,
    ) {
        use crate::worker::key_package_maintenance::sweep_expired;
        use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
        use openmls_traits::storage::StorageProvider;
        use xmtp_db::ConnectionExt;
        use xmtp_db::incoming_envelope::{IncomingLimits, PendingBudget};
        use xmtp_db::schema::{incoming_envelopes, key_package_history};
        use xmtp_proto::backend_v1::{client_envelope::Payload, welcome_message::Version};

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
        let cursor = welcome.cursor;
        pending_welcome_for_test(&bo.context, &welcome)
            .await
            .unwrap();
        let service = WelcomeService::new(bo.context.clone());
        let db = bo.context.db();

        let pending = db
            .pending_envelope(&service.topic(), cursor)
            .unwrap()
            .unwrap();
        let mut wire =
            xmtp_proto::backend_v1::ServerEnvelope::decode(pending.envelope.as_slice()).unwrap();
        let Payload::WelcomeMessage(message) =
            wire.envelope.as_mut().unwrap().payload.as_mut().unwrap()
        else {
            panic!("expected Welcome payload");
        };
        let Version::V1(message) = message.version.as_mut().unwrap() else {
            panic!("expected inline Welcome");
        };
        match failure {
            UnreadableWelcome::Wrapper => message.wrapper_algorithm = i32::MAX,
            UnreadableWelcome::MissingKey => message.hpke_public_key = vec![0; 32],
            UnreadableWelcome::MlsVersion => {
                (message.data, message.welcome_metadata) =
                    xmtp_mls_common::mls_ext::payload_encryption::wrap_payload_hpke(
                        &[0, 2],
                        &[],
                        &message.hpke_public_key,
                        xmtp_id::key_package::WrapperAlgorithm::try_from(message.wrapper_algorithm)
                            .unwrap(),
                        xmtp_configuration::WELCOME_HPKE_LABEL,
                    )
                    .unwrap();
            }
        }
        let row = || {
            incoming_envelopes::table.find((
                bo.context.installation_id().to_vec(),
                EntityKind::Welcome,
                cursor.0 as i64,
            ))
        };
        let envelope = wire.encode_to_vec();
        db.raw_query(|conn| {
            diesel::update(row())
                .set((
                    incoming_envelopes::envelope_bytes.eq(envelope.len() as i64),
                    incoming_envelopes::envelope.eq(&envelope),
                    incoming_envelopes::blocked.eq(previously_blocked),
                    incoming_envelopes::error_code
                        .eq(previously_blocked.then_some("welcome_blocked")),
                ))
                .execute(conn)
        })
        .unwrap();

        // First pass blocks the row and records a retention deadline.
        let before = xmtp_common::time::now_ns();
        if previously_blocked {
            service.retry_blocked_welcomes_after(Cursor(0)).unwrap();
        } else {
            service.process_pending_welcomes_once(&[]).unwrap();
        }
        let blocked = db
            .pending_envelope(&service.topic(), cursor)
            .unwrap()
            .unwrap();
        assert!(blocked.blocked);
        let deadline = blocked.unreadable_expires_at_ns.unwrap();
        assert!(deadline >= before + xmtp_common::NS_IN_DAY);
        assert!(deadline <= xmtp_common::time::now_ns() + xmtp_common::NS_IN_DAY);
        assert!(db.has_pending_welcomes().unwrap());

        // Before the deadline the row is retained, not removed.
        service.retry_blocked_welcomes_after(Cursor(0)).unwrap();
        assert!(
            db.pending_envelope(&service.topic(), cursor)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            db.pending_envelope(&service.topic(), cursor)
                .unwrap()
                .unwrap()
                .unreadable_expires_at_ns,
            Some(deadline),
            "a retry must not extend the first deadline"
        );

        let history = db
            .find_key_package_history_entries_before_id(i32::MAX)
            .unwrap()
            .pop()
            .unwrap();
        let hash = crate::identity::deserialize_key_package_hash_ref(&history.key_package_hash_ref)
            .unwrap();
        db.raw_query(|conn| {
            diesel::update(key_package_history::table.find(history.id))
                .set(key_package_history::delete_at_ns.eq(0))
                .execute(conn)
        })
        .unwrap();
        sweep_expired(&bo.context).unwrap();
        let retained: Option<openmls::prelude::KeyPackageBundle> =
            bo.context.mls_storage().key_package(&hash).unwrap();
        assert!(retained.is_some());

        // A held Welcome uses the held allowance, so one row of ready
        // capacity still admits the next Welcome.
        let next_group = alix.create_group(None, None).unwrap();
        next_group.invite(&bo).await.unwrap();
        let budget = PendingBudget {
            rows: 1,
            bytes: u64::MAX,
        };
        let limits = IncomingLimits {
            batch: budget,
            topic: budget,
            kind: budget,
        };
        let store = MlsStore::new(bo.context.clone());
        let topics = [Topic::new_welcome_message(bo.context.installation_id())];
        store.receive_topics_once(&topics, limits).await.unwrap();
        assert!(db.topic_progress(&service.topic()).unwrap().received > cursor);

        // At the deadline the final attempt runs and the row is removed, so the
        // Welcome barrier and key retirement are released.
        db.raw_query(|conn| {
            diesel::update(row())
                .set(incoming_envelopes::unreadable_expires_at_ns.eq(Some(1i64)))
                .execute(conn)
        })
        .unwrap();
        let outcomes = service.retry_blocked_welcomes_after(Cursor(0)).unwrap();
        let WelcomeHeadOutcome::Progress {
            result: Err(error), ..
        } = outcomes.into_iter().next().unwrap()
        else {
            panic!("expected terminal rejection");
        };
        match failure {
            UnreadableWelcome::Wrapper => {
                assert!(matches!(error, GroupError::UnsupportedWelcomeVersion(_)))
            }
            UnreadableWelcome::MlsVersion => assert!(matches!(
                error,
                GroupError::WelcomeError(openmls::prelude::WelcomeError::UnsupportedMlsVersion)
            )),
            UnreadableWelcome::MissingKey => assert!(matches!(
                error,
                GroupError::NotFound(xmtp_db::NotFound::KeyPackageReference(_))
            )),
        }
        assert_eq!(
            db.read_last_rejection(&service.topic())
                .unwrap()
                .unwrap()
                .code,
            "unsupported_welcome_expired"
        );
        assert!(
            db.pending_envelope(&service.topic(), cursor)
                .unwrap()
                .is_none()
        );
        assert!(db.topic_progress(&service.topic()).unwrap().processed >= cursor);
        assert!(
            db.welcome_barrier_complete(&service.topic(), cursor)
                .unwrap()
        );

        let next = db
            .first_pending_envelope(&service.topic())
            .unwrap()
            .unwrap();
        assert!(matches!(
            service
                .resolve_pending_welcome(Cursor(next.sequence_id as u64))
                .await
                .unwrap(),
            WelcomeHeadOutcome::Progress {
                result: Ok(Some(_)),
                ..
            }
        ));
        assert!(db.find_group(&next_group.group_id).unwrap().is_some());
        sweep_expired(&bo.context).unwrap();
        let removed: Option<openmls::prelude::KeyPackageBundle> =
            bo.context.mls_storage().key_package(&hash).unwrap();
        assert!(removed.is_none());
    }

    /// A group-prefix hold gets a deadline 3 days after the Welcome's backend
    /// timestamp and keeps key material until the hold ends. Classifying it
    /// past the deadline still holds it. Expiry needs the hold's row, a passed
    /// deadline, and a processed position at the target and below the anchor;
    /// it releases the Welcome barrier and the key package and leaves the
    /// held group unchanged.
    // verifies: JOIN-092
    #[xmtp_common::test(unwrap_try = true)]
    async fn group_prefix_wait_expires_after_three_days() {
        use crate::worker::key_package_maintenance::sweep_expired;
        use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
        use openmls_traits::storage::StorageProvider;
        use xmtp_db::ConnectionExt;
        use xmtp_db::schema::{incoming_envelopes, key_package_history};

        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let HeldReadd {
            bo_group, welcome, ..
        } = held_readd(&alix, &bo).await;
        let service = WelcomeService::new(bo.context.clone());
        let db = bo.context.db();
        let held = || -> Result<bool, GroupError> {
            Ok(db
                .pending_envelope(&service.topic(), welcome.cursor)?
                .is_some())
        };

        let WelcomeHeadOutcome::Need {
            requirement:
                WelcomeRequirement::GroupPrefix {
                    group_id,
                    anchor,
                    deadline_ns,
                },
            ..
        } = service.retry_pending_welcome(welcome.cursor, None)?
        else {
            panic!("expected a group-prefix hold");
        };
        assert_eq!(group_id, bo_group.group_id);
        assert_eq!(deadline_ns, welcome.timestamp() + WELCOME_RETENTION_NS);
        let row = db
            .pending_envelope(&service.topic(), welcome.cursor)?
            .unwrap();
        assert_eq!(row.error_code.as_deref(), Some("group_prefix"));
        assert!(!row.blocked);
        assert_eq!(row.retry_expires_at_ns, Some(deadline_ns));
        assert!(!db.welcome_barrier_complete(&service.topic(), welcome.cursor)?);
        let processed = db.topic_progress(&StreamTopic::group(group_id))?.processed;
        assert!(processed < anchor);
        let expire = |anchor, target| {
            service.expire_group_prefix_welcome(welcome.cursor, group_id, anchor, target)
        };

        let history = db
            .find_key_package_history_entries_before_id(i32::MAX)?
            .pop()
            .unwrap();
        let hash =
            crate::identity::deserialize_key_package_hash_ref(&history.key_package_hash_ref)?;
        db.raw_query(|conn| {
            diesel::update(
                key_package_history::table.filter(key_package_history::id.eq(history.id)),
            )
            .set(key_package_history::delete_at_ns.eq(0))
            .execute(conn)
        })?;
        sweep_expired(&bo.context)?;
        let retained: Option<openmls::prelude::KeyPackageBundle> =
            bo.context.mls_storage().key_package(&hash)?;
        assert!(
            retained.is_some(),
            "a prefix hold keeps expired private keys"
        );

        // Before the deadline the hold cannot expire.
        assert!(matches!(
            expire(anchor, processed)?,
            WelcomeHeadOutcome::Waiting { .. }
        ));
        assert!(held()?);

        // Past the deadline, a row that is no longer a group-prefix hold
        // cannot expire.
        db.raw_query(|conn| {
            diesel::update(incoming_envelopes::table.find((
                bo.context.installation_id().to_vec(),
                EntityKind::Welcome,
                welcome.cursor.0 as i64,
            )))
            .set((
                incoming_envelopes::retry_expires_at_ns.eq(Some(1i64)),
                incoming_envelopes::error_code.eq("identity_dependency"),
            ))
            .execute(conn)
        })?;
        assert!(matches!(
            expire(anchor, processed)?,
            WelcomeHeadOutcome::Waiting { .. }
        ));
        assert!(held()?);

        // Classification keeps the stored deadline and still holds the
        // Welcome until the coordinator decides.
        assert!(matches!(
            service.retry_pending_welcome(welcome.cursor, None)?,
            WelcomeHeadOutcome::Need {
                requirement: WelcomeRequirement::GroupPrefix { deadline_ns: 1, .. },
                ..
            }
        ));
        assert_eq!(
            db.pending_envelope(&service.topic(), welcome.cursor)?
                .unwrap()
                .retry_expires_at_ns,
            Some(1)
        );

        // Processing has not reached the target, or is not below the anchor.
        for (anchor, target) in [(anchor, Cursor(processed.0 + 1)), (processed, processed)] {
            assert!(matches!(
                expire(anchor, target)?,
                WelcomeHeadOutcome::Waiting { .. }
            ));
            assert!(held()?);
        }

        let epoch = bo_group.epoch().await?;
        assert!(matches!(
            expire(anchor, processed)?,
            WelcomeHeadOutcome::Progress {
                result: Err(GroupError::WelcomeGroupPrefixPending { .. }),
                ..
            }
        ));
        assert!(db.welcome_barrier_complete(&service.topic(), welcome.cursor)?);
        assert_eq!(
            db.read_last_rejection(&service.topic())?.unwrap().code,
            "group_prefix_expired"
        );
        assert!(!held()?);
        assert!(bo_group.is_active()?);
        assert_eq!(bo_group.epoch().await?, epoch);
        sweep_expired(&bo.context)?;
        let retired: Option<openmls::prelude::KeyPackageBundle> =
            bo.context.mls_storage().key_package(&hash)?;
        assert!(retired.is_none(), "key is retired once the hold ends");
    }

    enum WelcomeRetention {
        Unreadable,
        Held,
        Terminal,
    }

    /// Only unreadable MLS formats and missing Welcome key material get this
    /// deadline; local failures and required group versions keep their policy.
    // verifies: JOIN-087, JOIN-084, JOIN-085, JOIN-048, JOIN-027, JOIN-043
    #[rstest]
    #[case::mls_version(|| openmls::prelude::WelcomeError::UnsupportedMlsVersion.into(), WelcomeRetention::Unreadable)]
    #[case::extensions(|| openmls::prelude::WelcomeError::UnsupportedExtensions.into(), WelcomeRetention::Unreadable)]
    #[case::capability(|| openmls::prelude::WelcomeError::UnsupportedCapability.into(), WelcomeRetention::Unreadable)]
    #[case::ciphersuite(|| openmls::prelude::WelcomeError::UnsupportedCiphersuite(openmls::prelude::Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519).into(), WelcomeRetention::Unreadable)]
    #[case::key_reference(|| xmtp_db::NotFound::KeyPackageReference(vec![]).into(), WelcomeRetention::Unreadable)]
    #[case::key_package(|| xmtp_db::NotFound::KeyPackage(vec![]).into(), WelcomeRetention::Unreadable)]
    #[case::post_quantum_key(|| xmtp_db::NotFound::PostQuantumPrivateKey.into(), WelcomeRetention::Unreadable)]
    #[case::group_version(|| GroupError::UnsupportedWelcomeVersion("999.0.0".into()), WelcomeRetention::Held)]
    #[case::local_failure(|| GroupError::LockUnavailable, WelcomeRetention::Held)]
    #[case::unrelated_missing_state(|| xmtp_db::NotFound::MlsGroup(GroupId::from([1; 16])).into(), WelcomeRetention::Held)]
    #[case::psk(|| openmls::prelude::WelcomeError::Psk(openmls::schedule::errors::PskError::KeyNotFound).into(), WelcomeRetention::Terminal)]
    #[case::stale_join_anchor(|| xmtp_db::StorageError::Stream(xmtp_db::stream_storage::StreamStorageError::StaleJoinAnchor).into(), WelcomeRetention::Terminal)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn welcome_retention_is_limited_to_unreadable_input(
        #[case] error: fn() -> GroupError,
        #[case] expectation: WelcomeRetention,
        #[values(false, true)] prior_pointer_hold: bool,
    ) {
        use xmtp_db::ConnectionExt;
        use xmtp_db::incoming_envelope::NewIncomingEnvelope;

        tester!(bo, disable_workers);
        let service = WelcomeService::new(bo.context.clone());
        let db = bo.context.db();
        db.admit_ordered_batch(
            &service.topic(),
            Cursor(0),
            &[NewIncomingEnvelope {
                sequence_id: Cursor(1),
                envelope: vec![1],
            }],
            bo.context
                .incoming_runtime()
                .policy()
                .incoming_limits(NetworkEntityKind::Welcome),
        )
        .unwrap();
        if prior_pointer_hold {
            db.set_incoming_retry(
                &service.topic(),
                Cursor(1),
                &IncomingRetry {
                    retry_at_ns: 0,
                    blocked: false,
                    error_code: Some("welcome_pointee".into()),
                    retry_expires_at_ns: Some(1),
                },
            )
            .unwrap();
        }
        let pending = db
            .pending_envelope(&service.topic(), Cursor(1))
            .unwrap()
            .unwrap();
        let outcome = service
            .classify_pending_result(&pending, Err(error()))
            .unwrap();
        if matches!(expectation, WelcomeRetention::Terminal) {
            assert!(matches!(
                outcome,
                WelcomeHeadOutcome::Progress { result: Err(_), .. }
            ));
            let rejection = db.read_last_rejection(&service.topic()).unwrap();
            assert_eq!(rejection.unwrap().code, "invalid_welcome");
            assert!(
                db.pending_envelope(&service.topic(), Cursor(1))
                    .unwrap()
                    .is_none()
            );
            assert!(!db.has_pending_welcomes().unwrap());
            return;
        }
        assert!(matches!(outcome, WelcomeHeadOutcome::Waiting { .. }));
        let pending = db
            .pending_envelope(&service.topic(), Cursor(1))
            .unwrap()
            .unwrap();
        let bounded = matches!(expectation, WelcomeRetention::Unreadable);
        assert_eq!(pending.unreadable_expires_at_ns.is_some(), bounded);
        if bounded {
            assert!(pending.blocked);
            assert!(pending.unreadable_expires_at_ns.unwrap() > xmtp_common::time::now_ns());
            // Present the final attempt with a persisted elapsed deadline.
            db.raw_query(|conn| {
                use diesel::{ExpressionMethods, RunQueryDsl};
                use xmtp_db::schema::incoming_envelopes;
                diesel::update(incoming_envelopes::table)
                    .set(incoming_envelopes::unreadable_expires_at_ns.eq(Some(1i64)))
                    .execute(conn)
            })
            .unwrap();
            let pending = db
                .pending_envelope(&service.topic(), Cursor(1))
                .unwrap()
                .unwrap();
            assert!(matches!(
                service
                    .classify_pending_result(&pending, Err(error()))
                    .unwrap(),
                WelcomeHeadOutcome::Progress { result: Err(_), .. }
            ));
            assert!(!db.has_pending_welcomes().unwrap());
        }
    }

    /// A full held allowance gives up the oldest held Welcome, so new holds
    /// cannot grow held storage without bound.
    // verifies: PROC-020, JOIN-057
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_full_held_allowance_rejects_the_oldest_held_welcome() {
        use xmtp_db::incoming_envelope::NewIncomingEnvelope;

        tester!(bo, disable_workers);
        let mut settings = bo.context.incoming_runtime().policy().clone();
        settings.welcome_pending.rows = 2;
        let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .stream_policy(settings)
            .with_disable_workers(true)
            .with_allow_offline(Some(true))
            .build()
            .await?;
        let service = WelcomeService::new(client.context.clone());
        let db = client.context.db();
        let limits = client
            .context
            .incoming_runtime()
            .policy()
            .incoming_limits(NetworkEntityKind::Welcome);
        // Three Welcomes for groups that require a later client version.
        for sequence in 1..=3 {
            let envelope = NewIncomingEnvelope {
                sequence_id: Cursor(sequence),
                envelope: vec![1],
            };
            db.admit_ordered_batch(&service.topic(), Cursor(sequence - 1), &[envelope], limits)?;
            let pending = db
                .pending_envelope(&service.topic(), Cursor(sequence))?
                .unwrap();
            let floor = GroupError::UnsupportedWelcomeVersion("999.0.0".into());
            service.classify_pending_result(&pending, Err(floor))?;
        }
        assert!(db.pending_envelope(&service.topic(), Cursor(1))?.is_none());
        let rejection = db.read_last_rejection(&service.topic())?.unwrap();
        assert_eq!(
            (rejection.sequence_id, rejection.code.as_str()),
            (Cursor(1), "held_capacity")
        );
        for kept in [2, 3] {
            assert!(
                db.pending_envelope(&service.topic(), Cursor(kept))?
                    .unwrap()
                    .blocked
            );
        }
    }

    /// Gaining format support or restoring a missing key package must permit
    /// the final attempt to join, even after the unreadable deadline expires.
    // verifies: JOIN-087, JOIN-084
    #[rstest]
    #[case::wrapper("unsupported_welcome")]
    #[case::missing_key("welcome_blocked")]
    #[xmtp_common::test(unwrap_try = true)]
    async fn an_expired_unsupported_welcome_still_installs_once_it_is_supported(
        #[case] code: &'static str,
        #[values(false, true)] pointer: bool,
    ) {
        use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
        use openmls_traits::storage::StorageProvider;
        use xmtp_db::ConnectionExt;
        use xmtp_db::schema::incoming_envelopes;

        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let _other_installations = if pointer {
            tester!(bo2, from: bo, disable_workers);
            tester!(bo3, from: bo, disable_workers);
            Some((bo2, bo3))
        } else {
            None
        };
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
        assert_eq!(
            matches!(
                welcome.variant,
                xmtp_proto::types::WelcomeMessageType::WelcomePointer(_)
            ),
            pointer
        );
        let cursor = welcome.cursor;
        pending_welcome_for_test(&bo.context, &welcome)
            .await
            .unwrap();
        let service = WelcomeService::new(bo.context.clone());
        let db = bo.context.db();

        let missing_key = if code == "welcome_blocked" {
            let history = db
                .find_key_package_history_entries_before_id(i32::MAX)
                .unwrap()
                .pop()
                .unwrap();
            let hash_ref =
                crate::identity::deserialize_key_package_hash_ref(&history.key_package_hash_ref)
                    .unwrap();
            let storage = bo.context.mls_storage();
            let bundle: openmls::prelude::KeyPackageBundle =
                storage.key_package(&hash_ref).unwrap().unwrap();
            storage.delete_key_package(&hash_ref).unwrap();
            assert!(matches!(
                service.resolve_pending_welcome(cursor).await.unwrap(),
                WelcomeHeadOutcome::Waiting { blocked: true, .. }
            ));
            assert!(
                db.pending_envelope(&service.topic(), cursor)
                    .unwrap()
                    .unwrap()
                    .unreadable_expires_at_ns
                    .unwrap()
                    > xmtp_common::time::now_ns()
            );
            Some((hash_ref, bundle))
        } else {
            None
        };

        // Simulate the first retry after the unreadable deadline, then restore
        // the key package when it was missing. The wrapper case models a
        // client upgrade that now supports the stored Welcome.
        db.raw_query(|conn| {
            diesel::update(incoming_envelopes::table.find((
                bo.context.installation_id().to_vec(),
                EntityKind::Welcome,
                cursor.0 as i64,
            )))
            .set((
                incoming_envelopes::blocked.eq(true),
                incoming_envelopes::error_code.eq(Some(code)),
                incoming_envelopes::unreadable_expires_at_ns.eq(Some(1i64)),
            ))
            .execute(conn)
        })
        .unwrap();

        if let Some((hash_ref, bundle)) = missing_key {
            bo.context
                .mls_storage()
                .write_key_package(&hash_ref, &bundle)
                .unwrap();
        }

        assert!(matches!(
            service
                .retry_blocked_welcomes_after(Cursor(0))
                .unwrap()
                .as_slice(),
            [WelcomeHeadOutcome::Need { .. }]
        ));

        // The row must survive: an elapsed unsupported deadline must not delete
        // a Welcome this build can now read. It rejoins ordinary processing,
        // which may still need an async dependency before it installs.
        assert!(
            db.pending_envelope(&service.topic(), cursor)
                .unwrap()
                .is_some(),
            "a supported Welcome must not be deleted by a stale deadline"
        );
        assert!(db.topic_progress(&service.topic()).unwrap().processed < cursor);
        assert!(matches!(
            service.resolve_pending_welcome(cursor).await.unwrap(),
            WelcomeHeadOutcome::Progress {
                result: Ok(Some(_)),
                ..
            }
        ));
        assert!(db.find_group(&group.group_id).unwrap().is_some());
        assert!(!db.has_pending_welcomes().unwrap());
        assert!(db.read_last_rejection(&service.topic()).unwrap().is_none());
    }

    /// Legacy blocked pointers retain their classification while re-fetches
    /// miss, but the fixed pointer deadline still bounds missing data. At the
    /// cutoff, a real fetch can recover the join; a miss must release the row.
    /// Missing legacy deadlines use the pointer timestamp, never the retry time.
    // verifies: JOIN-094, JOIN-063, JOIN-035
    #[rstest]
    #[case::recovered(false)]
    #[case::still_missing(true)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn legacy_blocked_pointer_has_bounded_refetches(
        #[case] remains_missing: bool,
        #[values(false, true)] stored_deadline: bool,
    ) {
        use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
        use xmtp_db::{ConnectionExt, schema::incoming_envelopes};

        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        tester!(_bo2, from: bo, disable_workers);
        tester!(_bo3, from: bo, disable_workers);
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
        assert!(matches!(
            welcome.variant,
            xmtp_proto::types::WelcomeMessageType::WelcomePointer(_)
        ));
        pending_welcome_for_test(&bo.context, &welcome)
            .await
            .unwrap();
        let service = WelcomeService::new(bo.context.clone());
        let db = bo.context.db();
        let deadline = if stored_deadline {
            xmtp_common::time::now_ns() + xmtp_common::NS_IN_HOUR
        } else {
            welcome.timestamp() + WELCOME_RETENTION_NS
        };
        db.set_incoming_retry(
            &service.topic(),
            welcome.cursor,
            &IncomingRetry {
                retry_at_ns: 0,
                blocked: true,
                error_code: Some("welcome_blocked".into()),
                retry_expires_at_ns: stored_deadline.then_some(deadline),
            },
        )
        .unwrap();

        // Preserve the real pointer and keys while simulating a replica that
        // temporarily cannot return a pointee the old client had already read.
        let mut api = xmtp_api_backend::MockBackendClient::new();
        api.expect_query().returning(|_| {
            Ok(xmtp_proto::backend_v1::QueryResponse {
                envelopes: vec![],
                continuation: Some(xmtp_proto::backend_v1::Continuation { has_more: false }),
            })
        });
        let missing_client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .api_client(std::sync::Arc::new(api))
            .config_provider(std::sync::Arc::new(
                xmtp_configuration::StaticConfigProvider::default(),
            ))
            .with_allow_offline(Some(true))
            .with_disable_workers(true)
            .build()
            .await
            .unwrap();
        for _ in 0..2 {
            let missing_service = WelcomeService::new(missing_client.context.clone());
            assert!(matches!(
                missing_service
                    .retry_blocked_welcomes_after(Cursor(0))
                    .unwrap()
                    .as_slice(),
                [WelcomeHeadOutcome::Need { .. }]
            ));
            assert!(matches!(
                missing_service.resolve_pending_welcome(welcome.cursor).await.unwrap(),
                WelcomeHeadOutcome::Waiting { blocked: true, code, .. }
                    if code == "welcome_blocked"
            ));
            let pending = db
                .pending_envelope(&service.topic(), welcome.cursor)
                .unwrap()
                .unwrap();
            assert!(pending.blocked);
            assert_eq!(pending.error_code.as_deref(), Some("welcome_blocked"));
            assert_eq!(pending.retry_expires_at_ns, Some(deadline));
            assert_eq!(pending.unreadable_expires_at_ns, None);
            assert!(db.read_last_rejection(&service.topic()).unwrap().is_none());
            assert!(db.find_group(&group.group_id).unwrap().is_none());
        }

        db.raw_query(|conn| {
            diesel::update(incoming_envelopes::table.find((
                bo.context.installation_id().to_vec(),
                EntityKind::Welcome,
                welcome.cursor.0 as i64,
            )))
            .set(incoming_envelopes::retry_expires_at_ns.eq(Some(1i64)))
            .execute(conn)
        })
        .unwrap();
        assert!(matches!(
            service.retry_pending_welcome(welcome.cursor, None).unwrap(),
            WelcomeHeadOutcome::Need { .. }
        ));

        if remains_missing {
            let service = WelcomeService::new(missing_client.context.clone());
            assert!(matches!(
                service
                    .resolve_pending_welcome(welcome.cursor)
                    .await
                    .unwrap(),
                WelcomeHeadOutcome::Progress {
                    result: Err(GroupError::WelcomeDataNotFound(_)),
                    ..
                }
            ));
            assert!(!db.has_pending_welcomes().unwrap());
            assert!(db.find_group(&group.group_id).unwrap().is_none());
            assert_eq!(
                db.read_last_rejection(&service.topic())
                    .unwrap()
                    .unwrap()
                    .code,
                "welcome_pointer_expired"
            );
            return;
        }

        // Even past the fetch deadline, try a legacy blocked pointer before
        // rejecting it: it may now be readable after an upgrade or recovery.
        assert!(matches!(
            service
                .resolve_pending_welcome(welcome.cursor)
                .await
                .unwrap(),
            WelcomeHeadOutcome::Progress {
                result: Ok(Some(_)),
                ..
            }
        ));
        assert!(!db.has_pending_welcomes().unwrap());
        assert!(db.find_group(&group.group_id).unwrap().is_some());
        assert!(db.read_last_rejection(&service.topic()).unwrap().is_none());
    }

    /// How the backend fails a pointee query.
    #[derive(Clone, Copy, Debug)]
    enum PointeeQueryFailure {
        Unavailable,
        Denied,
        /// A row whose metadata the ordered read rejects. That is a backend
        /// fault, not a property of the pointee, unlike a first row whose
        /// payload does not decode.
        BadMetadata,
    }

    /// Query failures, including a response with invalid metadata, obey the
    /// same fixed fetch window as absent pointees, including rows persisted as
    /// welcome_retry by older clients. An existing unreadable hold keeps its
    /// own deadline through transport failures. A metadata fault is held like
    /// `Denied` once it is classified, so only a fresh row needs that case.
    // verifies: JOIN-094, JOIN-087, JOIN-063, JOIN-035
    #[rstest]
    #[case::fresh_unavailable(None, false, PointeeQueryFailure::Unavailable)]
    #[case::fresh_denied(None, false, PointeeQueryFailure::Denied)]
    #[case::fresh_bad_metadata(None, false, PointeeQueryFailure::BadMetadata)]
    #[case::legacy_retry_unavailable(
        Some("welcome_retry"),
        false,
        PointeeQueryFailure::Unavailable
    )]
    #[case::legacy_retry_denied(Some("welcome_retry"), false, PointeeQueryFailure::Denied)]
    #[case::unreadable_unavailable(Some("welcome_blocked"), true, PointeeQueryFailure::Unavailable)]
    #[case::unreadable_denied(Some("welcome_blocked"), true, PointeeQueryFailure::Denied)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn pointer_query_errors_have_bounded_retention(
        #[case] prior_code: Option<&'static str>,
        #[case] unreadable: bool,
        #[case] failure: PointeeQueryFailure,
    ) {
        use xmtp_proto::backend_v1 as wire;
        let retryable = matches!(failure, PointeeQueryFailure::Unavailable);
        use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
        use xmtp_db::{ConnectionExt, schema::incoming_envelopes};

        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        tester!(_bo2, from: bo, disable_workers);
        tester!(_bo3, from: bo, disable_workers);
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
        assert!(matches!(
            welcome.variant,
            xmtp_proto::types::WelcomeMessageType::WelcomePointer(_)
        ));
        pending_welcome_for_test(&bo.context, &welcome)
            .await
            .unwrap();
        let service = WelcomeService::new(bo.context.clone());
        let db = bo.context.db();
        let row = || {
            incoming_envelopes::table.find((
                bo.context.installation_id().to_vec(),
                EntityKind::Welcome,
                welcome.cursor.0 as i64,
            ))
        };
        let unreadable_deadline =
            unreadable.then(|| xmtp_common::time::now_ns() + xmtp_common::NS_IN_DAY);
        db.raw_query(|conn| {
            diesel::update(row())
                .set((
                    incoming_envelopes::error_code.eq(prior_code),
                    incoming_envelopes::blocked.eq(unreadable),
                    incoming_envelopes::retry_expires_at_ns.eq(unreadable.then_some(1i64)),
                    incoming_envelopes::unreadable_expires_at_ns.eq(unreadable_deadline),
                ))
                .execute(conn)
        })
        .unwrap();

        let mut api = xmtp_api_backend::MockBackendClient::new();
        api.expect_query().returning(move |request| {
            let status = match failure {
                PointeeQueryFailure::Unavailable => {
                    tonic::Status::unavailable("pointee query unavailable")
                }
                PointeeQueryFailure::Denied => {
                    tonic::Status::permission_denied("pointee query denied")
                }
                PointeeQueryFailure::BadMetadata => {
                    // The hash is one byte short of SHA-256.
                    return Ok(wire::QueryResponse {
                        envelopes: vec![wire::ServerEnvelope {
                            meta: Some(wire::EnvelopeMeta {
                                topic: request.queries[0].topic.clone(),
                                cursor: Some(wire::Cursor { sequence_id: 1 }),
                                message_hash: Some(wire::MessageHash {
                                    hash: Some(wire::message_hash::Hash::Sha256(vec![7; 31])),
                                }),
                                ..Default::default()
                            }),
                            envelope: None,
                        }],
                        continuation: Some(wire::Continuation { has_more: false }),
                    });
                }
            };
            Err(xmtp_proto::api::ApiClientError::client(
                xmtp_api_grpc::error::GrpcError::Status(status),
            ))
        });
        let failing_client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .api_client(std::sync::Arc::new(api))
            .config_provider(std::sync::Arc::new(
                xmtp_configuration::StaticConfigProvider::default(),
            ))
            .with_allow_offline(Some(true))
            .with_disable_workers(true)
            .build()
            .await
            .unwrap();
        let failing_service = WelcomeService::new(failing_client.context.clone());
        let blocked = unreadable || !retryable;
        assert!(matches!(
            failing_service.resolve_pending_welcome(welcome.cursor).await.unwrap(),
            WelcomeHeadOutcome::Waiting { blocked: actual, .. } if actual == blocked
        ));
        let pending = db
            .pending_envelope(&service.topic(), welcome.cursor)
            .unwrap()
            .unwrap();
        assert_eq!(
            pending.error_code.as_deref(),
            Some(if blocked {
                "welcome_blocked"
            } else {
                "welcome_pointee"
            })
        );
        assert_eq!(
            pending.retry_expires_at_ns,
            Some(if unreadable {
                1
            } else {
                welcome.timestamp() + WELCOME_RETENTION_NS
            })
        );
        assert_eq!(pending.unreadable_expires_at_ns, unreadable_deadline);
        assert!(db.read_last_rejection(&service.topic()).unwrap().is_none());

        db.raw_query(|conn| {
            diesel::update(row())
                .set((
                    incoming_envelopes::retry_expires_at_ns.eq(Some(1i64)),
                    incoming_envelopes::unreadable_expires_at_ns.eq(unreadable.then_some(1i64)),
                ))
                .execute(conn)
        })
        .unwrap();
        let failing_service = WelcomeService::new(failing_client.context.clone());
        assert!(matches!(
            failing_service
                .resolve_pending_welcome(welcome.cursor)
                .await
                .unwrap(),
            WelcomeHeadOutcome::Progress { result: Err(_), .. }
        ));
        assert!(!db.has_pending_welcomes().unwrap());
        assert!(db.find_group(&group.group_id).unwrap().is_none());
        assert_eq!(
            db.read_last_rejection(&service.topic())
                .unwrap()
                .unwrap()
                .code,
            if unreadable {
                "unsupported_welcome_expired"
            } else {
                "welcome_pointer_expired"
            }
        );
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
        let envelope = wire.encode_to_vec();
        db.raw_query(|conn| {
            diesel::update(incoming_envelopes::table.find((
                bo.context.installation_id().to_vec(),
                EntityKind::Welcome,
                first.0 as i64,
            )))
            .set((
                incoming_envelopes::envelope_bytes.eq(envelope.len() as i64),
                incoming_envelopes::envelope.eq(&envelope),
            ))
            .execute(conn)
        })?;

        let outcomes = service.process_pending_welcomes_once(&[])?;
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
