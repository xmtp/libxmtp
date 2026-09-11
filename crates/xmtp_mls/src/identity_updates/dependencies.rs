//! Exact, verified identity proofs for synchronous MLS state changes.

use super::{get_association_state_with_verifier, load_identity_updates};
use crate::{client::ClientError, context::XmtpSharedContext};
use futures::{StreamExt, stream};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock, Weak},
};
use thiserror::Error;
use xmtp_common::{
    RetryableError, retryable,
    time::{Duration, Instant, sleep},
};
use xmtp_configuration::IDENTITY_REFERENCE_RETRY_INTERVAL;
use xmtp_db::{DbQuery, StorageError, prelude::*};
use xmtp_id::associations::{AssociationError, AssociationState};

/// A proof at one exact identity sequence. Zero is not an identity sequence.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct IdentityRequirement {
    /// Inbox whose verified history supplies this proof.
    pub inbox_id: String,
    /// Exact historical identity update, never a request for the latest state.
    pub sequence_id: u64,
}

/// Separate a missing cached proof from invalid or unavailable identity history.
#[derive(Debug, Error)]
pub enum IdentityDependencyError {
    /// Retry outside the state transaction after obtaining this exact proof.
    #[error("A verified identity proof is required at sequence {}", .0.sequence_id)]
    Need(IdentityRequirement),
    /// The reference cannot name a stored identity update. Not retryable.
    #[error("Invalid identity sequence {0}")]
    InvalidSequence(u64),
    /// The reference stayed absent for the healthy-primary wait. Not retryable.
    #[error("Identity reference is absent at sequence {}", .0.sequence_id)]
    MissingReference(IdentityRequirement),
    /// A proof or its retrieval failed. Invalid identity history blocks dependents.
    #[error(transparent)]
    Client(#[from] Box<ClientError>),
    /// A local proof could not be read. Retryability follows the storage error.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// Concurrent callers observe the same failed proof attempt.
    #[error(transparent)]
    Shared(Arc<IdentityDependencyError>),
}

impl From<ClientError> for IdentityDependencyError {
    fn from(error: ClientError) -> Self {
        Self::Client(Box::new(error))
    }
}

impl RetryableError for IdentityDependencyError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Need(_) => true,
            Self::Client(error) => retryable!(error),
            Self::Storage(error) => retryable!(error),
            Self::Shared(error) => error.is_retryable(),
            Self::InvalidSequence(_) | Self::MissingReference(_) => false,
        }
    }
}

type Resolution = tokio::sync::Mutex<Option<Result<(), Arc<IdentityDependencyError>>>>;

impl crate::worker::NeedsDbReconnect for IdentityDependencyError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Client(error) => error.db_needs_connection(),
            Self::Storage(error) => error.db_needs_connection(),
            Self::Shared(error) => error.needs_db_reconnect(),
            Self::Need(_) | Self::InvalidSequence(_) | Self::MissingReference(_) => false,
        }
    }
}

/// Share concurrent proof attempts for one client and bound network requests.
#[derive(Default)]
pub struct IdentityResolutionRegistry {
    /// Weak entries disappear after all callers release an attempt.
    active: parking_lot::Mutex<HashMap<IdentityRequirement, Weak<Resolution>>>,
    /// One request limit shared by single-proof and batch callers.
    permits: OnceLock<Arc<tokio::sync::Semaphore>>,
}

impl IdentityResolutionRegistry {
    fn permits(&self, limit: usize) -> Arc<tokio::sync::Semaphore> {
        self.permits
            .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(limit)))
            .clone()
    }

    /// Reuse the current exact-proof attempt without caching its failure forever.
    fn acquire(&self, requirement: &IdentityRequirement) -> Arc<Resolution> {
        let mut active = self.active.lock();
        active.retain(|_, gate| gate.strong_count() != 0);
        if let Some(gate) = active.get(requirement).and_then(Weak::upgrade) {
            return gate;
        }
        let gate = Arc::new(Resolution::default());
        active.insert(requirement.clone(), Arc::downgrade(&gate));
        gate
    }
}

impl IdentityRequirement {
    fn checked_sequence(&self) -> Result<i64, IdentityDependencyError> {
        i64::try_from(self.sequence_id)
            .ok()
            .filter(|sequence| *sequence > 0)
            .ok_or(IdentityDependencyError::InvalidSequence(self.sequence_id))
    }
}

/// Read only a verified cached snapshot. Never resolve to a newer snapshot.
/// The caller supplies the state transaction's connection.
pub(crate) fn require_association_state(
    conn: &impl DbQuery,
    requirement: &IdentityRequirement,
) -> Result<AssociationState, IdentityDependencyError> {
    let sequence = requirement.checked_sequence()?;
    let state = conn
        .read_from_cache(&requirement.inbox_id, sequence)?
        .ok_or_else(|| IdentityDependencyError::Need(requirement.clone()))?;
    if state.inbox_id != requirement.inbox_id {
        return Err(StorageError::DbDeserialize.into());
    }
    state
        .try_into()
        .map_err(StorageError::from)
        .map_err(Into::into)
}

/// Fetch and verify an exact identity proof without a state transaction.
/// Successful primary reads must cover the full absence interval. A transport
/// or verification error stops the interval and leaves dependent work pending.
/// Concurrent callers share one attempt; cancellation permits a later retry.
pub(crate) async fn resolve_identity_requirement(
    context: &impl XmtpSharedContext,
    requirement: &IdentityRequirement,
) -> Result<(), IdentityDependencyError> {
    match require_association_state(&context.db(), requirement) {
        Ok(_) => return Ok(()),
        Err(IdentityDependencyError::Need(_)) => {}
        Err(error) => return Err(error),
    }
    let gate = context.identity_resolution_registry().acquire(requirement);
    let mut resolution = gate.lock().await;
    if resolution.is_none() {
        let permits = context
            .identity_resolution_registry()
            .permits(context.incoming_runtime().policy().max_dependency_requests);
        let _permit = permits
            .acquire()
            .await
            .expect("identity request semaphore stays open");
        *resolution = Some(
            resolve_identity_requirement_with_wait(
                context,
                requirement,
                context.incoming_runtime().policy().identity_reference_wait,
            )
            .await
            .map_err(Arc::new),
        );
    }
    match resolution.as_ref().expect("resolution is assigned") {
        Ok(()) => Ok(()),
        Err(error) => Err(match error.as_ref() {
            IdentityDependencyError::MissingReference(requirement) => {
                IdentityDependencyError::MissingReference(requirement.clone())
            }
            IdentityDependencyError::InvalidSequence(sequence) => {
                IdentityDependencyError::InvalidSequence(*sequence)
            }
            _ => IdentityDependencyError::Shared(error.clone()),
        }),
    }
}

/// Declare absence only after healthy primary reads span the configured wait.
/// Verify the available prefix first; invalid history is not an absent reference.
async fn resolve_identity_requirement_with_wait(
    context: &impl XmtpSharedContext,
    requirement: &IdentityRequirement,
    absence_wait: Duration,
) -> Result<(), IdentityDependencyError> {
    let sequence = requirement.checked_sequence()?;
    let mut healthy_since = None;
    loop {
        let conn = context.db();
        match require_association_state(&conn, requirement) {
            Ok(_) => return Ok(()),
            Err(IdentityDependencyError::Need(_)) => {}
            Err(error) => return Err(error),
        }
        // Query uses the primary and returns the complete retained prefix.
        load_identity_updates(context.api(), &conn, &[requirement.inbox_id.as_str()]).await?;
        match get_association_state_with_verifier(
            &conn,
            &requirement.inbox_id,
            Some(sequence),
            &context.scw_verifier(),
        )
        .await
        {
            Ok(_) => return Ok(()),
            Err(ClientError::Association(AssociationError::MissingIdentityUpdate)) => {
                // An invalid earlier update blocks this dependency. Absence
                // alone must not bypass verification of the available prefix.
                let last = conn
                    .get_identity_updates(&requirement.inbox_id, None, Some(sequence))
                    .map_err(ClientError::from)?
                    .last()
                    .map(|update| update.sequence_id);
                if let Some(last) = last {
                    get_association_state_with_verifier(
                        &conn,
                        &requirement.inbox_id,
                        Some(last),
                        &context.scw_verifier(),
                    )
                    .await?;
                }
            }
            Err(error) => return Err(error.into()),
        }
        let start = healthy_since.get_or_insert_with(Instant::now);
        if start.elapsed() >= absence_wait {
            return Err(IdentityDependencyError::MissingReference(
                requirement.clone(),
            ));
        }
        drop(conn);
        sleep(IDENTITY_REFERENCE_RETRY_INTERVAL).await;
    }
}

/// Coalesce exact requirements and keep every completed result. One failed
/// proof does not cancel other fetches or remove their verified cache entries.
pub(crate) async fn resolve_identity_requirements(
    context: &impl XmtpSharedContext,
    requirements: impl IntoIterator<Item = IdentityRequirement>,
) -> Vec<(IdentityRequirement, Result<(), IdentityDependencyError>)> {
    let requirements: HashSet<_> = requirements.into_iter().collect();
    stream::iter(requirements.into_iter().map(|requirement| async move {
        let result = resolve_identity_requirement(context, &requirement).await;
        (requirement, result)
    }))
    .buffer_unordered(context.incoming_runtime().policy().max_dependency_requests)
    .collect()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tester;

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    fn identity_and_query_futures_are_send() {
        fn assert_send<T: Send>(_: T) {}
        let context = crate::test::mock::context();
        let requirement = IdentityRequirement {
            inbox_id: "inbox".into(),
            sequence_id: 1,
        };
        assert_send(resolve_identity_requirement(&context, &requirement));
        assert_send(resolve_identity_requirements(
            &context,
            [requirement.clone()],
        ));
        assert_send(context.api().newest_topic_cursors(vec![]));
        assert_send(context.api().query_ordered_page(
            Default::default(),
            1,
            xmtp_proto::types::IncomingBatchLimits {
                max_rows: 1,
                max_bytes: 1024,
            },
        ));
        // Check the production transport type as well as the mock type.
        let _: fn(&crate::Client<crate::MlsContext>) = |client| {
            assert_send(async move { client.inbox_state(true).await });
            assert_send(xmtp_common::bind_task_hub(async move {
                client.inbox_state(true).await
            }));
            assert_send(async move {
                client
                    .wait_for_registration_visible(Default::default())
                    .await
            });
            assert_send(async move { client.context.api().get_inbox_ids(vec![]).await });
            assert_send(async move { client.context.api().newest_topic_cursors(vec![]).await });
        };
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn identity_requests_share_one_context_limit() {
        let registry = IdentityResolutionRegistry::default();
        let first = registry.permits(1);
        let second = registry.permits(1);
        assert!(Arc::ptr_eq(&first, &second));
        let permit = first.acquire().await?;
        assert!(second.try_acquire().is_err());
        drop(permit);
        assert!(second.try_acquire().is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn concurrent_requirements_share_one_attempt_but_later_calls_retry() {
        let registry = IdentityResolutionRegistry::default();
        let requirement = IdentityRequirement {
            inbox_id: "inbox".into(),
            sequence_id: 5,
        };
        let first = registry.acquire(&requirement);
        let second = registry.acquire(&requirement);
        assert!(Arc::ptr_eq(&first, &second));

        let mut resolution = first.lock().await;
        assert!(second.try_lock().is_err());
        *resolution = Some(Err(Arc::new(IdentityDependencyError::InvalidSequence(5))));
        drop(resolution);
        assert!(matches!(second.lock().await.as_ref(), Some(Err(_))));

        let completed = Arc::downgrade(&first);
        drop(first);
        drop(second);
        let next = registry.acquire(&requirement);
        assert!(completed.upgrade().is_none());
        assert!(next.lock().await.is_none());
        assert_eq!(registry.active.lock().len(), 1);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn historical_proof_does_not_use_a_newer_cached_snapshot() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let first_sequence =
            alix.context
                .db()
                .get_latest_sequence_id(&[alix.inbox_id()])?[alix.inbox_id()] as u64;
        tester!(alix2, from: alix, disable_workers);
        load_identity_updates(alix.context.api(), &alix.context.db(), &[alix.inbox_id()]).await?;
        let last_sequence =
            alix.context
                .db()
                .get_latest_sequence_id(&[alix.inbox_id()])?[alix.inbox_id()] as u64;
        assert!(last_sequence > first_sequence);
        let latest = IdentityRequirement {
            inbox_id: alix.inbox_id().to_owned(),
            sequence_id: last_sequence,
        };
        resolve_identity_requirement(&bo.context, &latest).await?;
        let historical = IdentityRequirement {
            sequence_id: first_sequence,
            ..latest
        };
        assert!(matches!(
            require_association_state(&bo.context.db(), &historical),
            Err(IdentityDependencyError::Need(_))
        ));
        resolve_identity_requirement(&bo.context, &historical).await?;
        let state = require_association_state(&bo.context.db(), &historical)?;
        assert!(
            state
                .installation_ids()
                .contains(&alix.installation_public_key().to_vec())
        );
        assert!(
            !state
                .installation_ids()
                .contains(&alix2.installation_public_key().to_vec())
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn dependency_batch_keeps_success_after_an_invalid_sibling() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let sequence_id =
            alix.context
                .db()
                .get_latest_sequence_id(&[alix.inbox_id()])?[alix.inbox_id()] as u64;
        let valid = IdentityRequirement {
            inbox_id: alix.inbox_id().to_owned(),
            sequence_id,
        };
        let invalid = IdentityRequirement {
            sequence_id: 0,
            ..valid.clone()
        };
        let results =
            resolve_identity_requirements(&bo.context, [valid.clone(), invalid, valid.clone()])
                .await;
        assert_eq!(results.len(), 2);
        assert_eq!(
            results.iter().filter(|(_, result)| result.is_ok()).count(),
            1
        );
        assert!(require_association_state(&bo.context.db(), &valid).is_ok());
        assert!(
            results.iter().any(|(_, result)| matches!(
                result,
                Err(IdentityDependencyError::InvalidSequence(0))
            ))
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn primary_absence_is_terminal_only_after_successful_query() {
        tester!(alix, disable_workers);
        let requirement = IdentityRequirement {
            inbox_id: hex::encode(xmtp_common::rand_vec::<32>()),
            sequence_id: 1,
        };
        let result =
            resolve_identity_requirement_with_wait(&alix.context, &requirement, Duration::ZERO)
                .await;
        assert!(
            matches!(result, Err(IdentityDependencyError::MissingReference(found)) if found == requirement)
        );
    }
}
