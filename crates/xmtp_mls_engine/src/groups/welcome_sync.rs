//! Durable Welcome processing for the incoming coordinator and client sync APIs.
//!
//! `WelcomeService` installs independent joins, resolves their dependencies, and
//! holds Welcomes with unsupported formats, unavailable keys, unresolved pointers,
//! or unreached group prefixes for a bounded window.
//! `WelcomeHeadOutcome` tells the coordinator which work remains; `GroupSyncSummary`
//! reports completed group synchronization.

use crate::groups::InitialMembershipValidator;
#[cfg(test)]
use crate::groups::ValidateGroupMembership;
use crate::groups::XmtpWelcome;
use crate::groups::context::XmtpSharedContext;
use crate::groups::mls_ext::ResolvedWelcome;
use crate::groups::{GroupError, MlsGroup};
#[cfg(any(test, feature = "test-utils"))]
use crate::intents::ProcessIntentError;
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
#[cfg(any(test, feature = "test-utils"))]
use xmtp_mls_core::mls_store::MlsStore;
use xmtp_mls_identity::identity_updates::{
    IdentityDependencyError, IdentityRequirement, InstallationDiffError,
    resolve_identity_requirements,
};
use xmtp_proto::types::Topic;
use xmtp_proto::types::{Cursor, GroupId};

mod group_sync;
#[cfg(any(test, feature = "test-utils"))]
mod test_support;

#[cfg(any(test, feature = "test-utils"))]
pub use test_support::pending_welcome_for_test;
#[cfg(test)]
pub(crate) use test_support::{HeldReadd, held_readd};

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
                use xmtp_db::DuplicateItem::*;
                use xmtp_db::StorageError::*;

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
                self.defer_pending(pending, "identity_dependency", false, None, None)?;
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
                self.defer_pending(pending, "group_prefix", false, Some(deadline_ns), None)?;
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
                self.defer_pending(pending, code, blocked, None, error.rate_limit_delay())?;
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
        self.defer_pending(pending, code, true, None, None)?;
        Ok(WelcomeHeadOutcome::Waiting {
            cursor,
            code: code.into(),
            blocked: true,
        })
    }

    /// Keep retry state without extending the original expiry deadline. A
    /// rate-limited request is not retried before `floor`, its delay, but never
    /// later than `deadline`: an expired pointer is then rejected without a
    /// fetch, so a delay longer than its retention cannot stall it.
    // implements: RATE-041
    fn defer_pending(
        &self,
        pending: &StoredIncomingEnvelope,
        code: &'static str,
        blocked: bool,
        deadline: Option<i64>,
        floor: Option<xmtp_common::time::Duration>,
    ) -> Result<(), GroupError> {
        let policy = self.context.incoming_runtime().policy();
        let delay = policy
            .active_database_poll_interval
            .max(floor.unwrap_or_default());
        self.context.db().set_incoming_retry(
            &self.topic(),
            Cursor(pending.sequence_id as u64),
            &IncomingRetry {
                retry_at_ns: xmtp_common::time::now_ns()
                    .saturating_add(i64::try_from(delay.as_nanos()).unwrap_or(i64::MAX))
                    .min(deadline.unwrap_or(i64::MAX)),
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
        #[cfg(any(test, feature = "diagnostics"))]
        crate::groups::diagnostics::record_welcome_retry(code);
        Ok(())
    }

    /// Commit rejection progress only if the exact pending bytes still match.
    fn complete_rejected_pending(
        &self,
        pending: &StoredIncomingEnvelope,
        code: &'static str,
    ) -> Result<(), GroupError> {
        xmtp_mls_core::state_tx::state_write(self.context.mls_storage(), |tx| {
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
        let expiry = xmtp_mls_core::state_tx::state_write(self.context.mls_storage(), |tx| {
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
                self.defer_pending(
                    &pending,
                    code,
                    blocked,
                    Some(deadline),
                    error.rate_limit_delay(),
                )?;
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
}

#[cfg(test)]
mod tests;
