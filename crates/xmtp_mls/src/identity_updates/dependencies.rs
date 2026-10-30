//! Exact, verified identity proofs for synchronous MLS state changes.

#[cfg(test)]
use super::load_identity_updates;
use super::{get_association_state_for_client, load_identity_updates_for_client};
use crate::{client::ClientError, context::XmtpSharedContext};
use futures::{StreamExt, stream};
use std::{
    collections::{BTreeSet, HashSet},
    sync::{Arc, OnceLock},
};
use thiserror::Error;
use xmtp_common::{
    RetryableError, retryable,
    time::{Instant, sleep},
};
use xmtp_configuration::{
    IDENTITY_REFERENCE_MAX_RETRY_INTERVAL, IDENTITY_REFERENCE_RETRY_INTERVAL,
};
use xmtp_db::{DbQuery, StorageError};
use xmtp_id::associations::{AssociationError, AssociationState};

/// A proof at one exact identity sequence. Zero is not an identity sequence.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IdentityRequirement {
    /// Inbox whose verified history supplies this proof.
    pub inbox_id: String,
    /// Exact historical identity update, never a request for the latest state.
    pub sequence_id: u64,
}

/// Separate a missing cached proof from invalid or unavailable identity history.
#[derive(Debug, Error)]
pub enum IdentityDependencyError {
    /// Retry outside the state transaction after obtaining these exact proofs.
    /// The set holds every missing proof the attempt found, never zero proofs.
    #[error("Verified identity proofs are required at {} references", .0.len())]
    Need(BTreeSet<IdentityRequirement>),
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
            Self::InvalidSequence(_) | Self::MissingReference(_) => false,
        }
    }

    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        match self {
            Self::Client(error) => error.rate_limit_delay(),
            _ => None,
        }
    }
}

impl crate::worker::NeedsDbReconnect for IdentityDependencyError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Client(error) => error.db_needs_connection(),
            Self::Storage(error) => error.db_needs_connection(),
            Self::Need(_) | Self::InvalidSequence(_) | Self::MissingReference(_) => false,
        }
    }
}

/// Bound identity proof requests for one client.
#[derive(Default)]
pub struct IdentityResolutionRegistry {
    /// One request limit shared by every resolution round.
    permits: OnceLock<Arc<tokio::sync::Semaphore>>,
    /// Serializes own-inbox loads and their event emission for one client.
    own_inbox_refresh: tokio::sync::Mutex<OwnInstallationEvents>,
}

/// How far this client has reported changes to its own installations.
#[derive(Debug, Default)]
pub(crate) struct OwnInstallationEvents {
    /// The own-inbox identity update through which changes are emitted. A
    /// built client starts at the latest one stored then. `None` while no
    /// own-inbox update is stored; the first own-inbox load then starts at
    /// the latest one stored without emitting for it.
    pub(crate) emitted_through: Option<i64>,
}

impl IdentityResolutionRegistry {
    /// A registry whose own-installation events start after `emitted_through`.
    pub(crate) fn starting_after(emitted_through: Option<i64>) -> Self {
        Self {
            own_inbox_refresh: tokio::sync::Mutex::new(OwnInstallationEvents { emitted_through }),
            ..Default::default()
        }
    }

    pub(crate) fn own_inbox_refresh(&self) -> &tokio::sync::Mutex<OwnInstallationEvents> {
        &self.own_inbox_refresh
    }

    fn permits(&self, limit: usize) -> Arc<tokio::sync::Semaphore> {
        self.permits
            .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(limit)))
            .clone()
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
// implements: IDENT-070
pub(crate) fn require_association_state(
    conn: &impl DbQuery,
    requirement: &IdentityRequirement,
) -> Result<AssociationState, IdentityDependencyError> {
    let sequence = requirement.checked_sequence()?;
    let state = conn
        .read_from_cache(&requirement.inbox_id, sequence)?
        .ok_or_else(|| IdentityDependencyError::Need(BTreeSet::from([requirement.clone()])))?;
    if state.inbox_id != requirement.inbox_id {
        return Err(StorageError::DbDeserialize.into());
    }
    state
        .try_into()
        .map_err(StorageError::from)
        .map_err(Into::into)
}

/// Read cached proofs and record each missing one, so that one resolution
/// fetches every proof an attempt lacks instead of one proof per attempt.
#[derive(Default)]
pub(crate) struct MissingProofs(BTreeSet<IdentityRequirement>);

impl MissingProofs {
    /// Return the cached state, or record the requirement and return `None`.
    /// Other failures do not depend on a fetch, so they return at once.
    pub(crate) fn read(
        &mut self,
        conn: &impl DbQuery,
        requirement: &IdentityRequirement,
    ) -> Result<Option<AssociationState>, IdentityDependencyError> {
        match require_association_state(conn, requirement) {
            Ok(state) => Ok(Some(state)),
            Err(IdentityDependencyError::Need(needed)) => {
                self.0.extend(needed);
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// Fail with every recorded requirement.
    pub(crate) fn check(self) -> Result<(), IdentityDependencyError> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(IdentityDependencyError::Need(self.0))
        }
    }
}

/// Resolve every requirement of one parent and return one result for it.
/// Each round loads the logs of every still-missing inbox in one request and
/// checks each proof once, so absent references share one window. Successful
/// primary reads must cover the full absence interval. A transport or
/// verification error stops the interval and leaves dependent work pending.
/// One failed proof does not cancel other checks or remove their verified
/// cache entries.
// implements: PROC-013
pub(crate) async fn resolve_identity_requirements(
    context: &impl XmtpSharedContext,
    requirements: &BTreeSet<IdentityRequirement>,
) -> Result<(), IdentityDependencyError> {
    let policy = context.incoming_runtime().policy();
    let limit = policy.max_dependency_requests;
    let permits = context.identity_resolution_registry().permits(limit);
    let mut missing = requirements.clone();
    let mut healthy_since = None;
    let mut interval = IDENTITY_REFERENCE_RETRY_INTERVAL;
    loop {
        let conn = context.db();
        let mut results = Vec::new();
        let mut needed = Vec::new();
        for requirement in &missing {
            match require_association_state(&conn, requirement) {
                Err(IdentityDependencyError::Need(_)) => needed.push(requirement.clone()),
                result => results.push(result.map(drop)),
            }
        }
        if !needed.is_empty() {
            // The permit covers one load and its checks, never a sleep.
            let _permit = permits
                .acquire()
                .await
                .expect("identity request semaphore stays open");
            let inbox_ids: Vec<_> = needed
                .iter()
                .map(|requirement| requirement.inbox_id.as_str())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            // Query uses the primary and returns the complete retained prefix.
            load_identity_updates_for_client(context, &conn, &inbox_ids).await?;
            let conn = &conn;
            let checked: Vec<_> = stream::iter(needed.into_iter().map(|requirement| async move {
                check_identity_requirement(context, conn, &requirement).await
            }))
            .buffer_unordered(limit)
            .collect()
            .await;
            results.extend(checked);
        }
        missing = results
            .iter()
            .filter_map(|result| match result {
                Err(IdentityDependencyError::MissingReference(requirement)) => {
                    Some(requirement.clone())
                }
                _ => None,
            })
            .collect();
        match parent_result(results) {
            Err(IdentityDependencyError::MissingReference(requirement)) => {
                let start = healthy_since.get_or_insert_with(Instant::now);
                if start.elapsed() >= policy.identity_reference_wait {
                    return Err(IdentityDependencyError::MissingReference(requirement));
                }
            }
            result => return result,
        }
        drop(conn);
        sleep(interval).await;
        interval = (interval * 2).min(IDENTITY_REFERENCE_MAX_RETRY_INTERVAL);
    }
}

/// Verify one exact proof after a primary load. Verify the available prefix
/// first; invalid history is not an absent reference.
async fn check_identity_requirement(
    context: &impl XmtpSharedContext,
    conn: &impl DbQuery,
    requirement: &IdentityRequirement,
) -> Result<(), IdentityDependencyError> {
    let sequence = requirement.checked_sequence()?;
    match get_association_state_for_client(context, conn, &requirement.inbox_id, Some(sequence))
        .await
    {
        Ok(_) => Ok(()),
        Err(ClientError::Association(AssociationError::MissingIdentityUpdate)) => {
            // An invalid earlier update blocks this dependency. Absence
            // alone must not bypass verification of the available prefix.
            let last = conn
                .get_identity_updates(&requirement.inbox_id, None, Some(sequence))
                .map_err(ClientError::from)?
                .last()
                .map(|update| update.sequence_id);
            if let Some(last) = last {
                get_association_state_for_client(context, conn, &requirement.inbox_id, Some(last))
                    .await?;
            }
            Err(IdentityDependencyError::MissingReference(
                requirement.clone(),
            ))
        }
        Err(error) => Err(error.into()),
    }
}

/// A failed proof takes precedence over an absent one, so a parent that
/// needs unresolved identity history stays blocked, not rejected. Of the
/// failed proofs, the rate-limited one with the longest delay is returned, so
/// the parent's retry waits out every refused verification instead of
/// resending one inside its own delay.
// implements: PROC-012, RATE-041
fn parent_result(
    results: impl IntoIterator<Item = Result<(), IdentityDependencyError>>,
) -> Result<(), IdentityDependencyError> {
    let mut absent = None;
    let mut failed: Option<IdentityDependencyError> = None;
    for result in results {
        match result {
            Ok(()) => {}
            Err(error @ IdentityDependencyError::MissingReference(_)) => absent = Some(error),
            // `None` orders first, so any delay beats none and a longer one
            // beats a shorter one; the first failure is kept on a tie.
            Err(error) => {
                if failed
                    .as_ref()
                    .is_none_or(|kept| error.rate_limit_delay() > kept.rate_limit_delay())
                {
                    failed = Some(error);
                }
            }
        }
    }
    failed.or(absent).map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tester;
    use xmtp_common::time::Duration;
    use xmtp_db::prelude::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    fn identity_and_query_futures_are_send() {
        fn assert_send<T: Send>(_: T) {}
        let context = crate::test::mock::context();
        let requirement = IdentityRequirement {
            inbox_id: "inbox".into(),
            sequence_id: 1,
        };
        assert_send(resolve_identity_requirements(
            &context,
            &BTreeSet::from([requirement.clone()]),
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

    // verifies: IDENT-070
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
        resolve_identity_requirements(&bo.context, &BTreeSet::from([latest.clone()])).await?;
        let historical = IdentityRequirement {
            sequence_id: first_sequence,
            ..latest
        };
        assert!(matches!(
            require_association_state(&bo.context.db(), &historical),
            Err(IdentityDependencyError::Need(_))
        ));
        resolve_identity_requirements(&bo.context, &BTreeSet::from([historical.clone()])).await?;
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

    // verifies: PROC-013
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
        let result =
            resolve_identity_requirements(&bo.context, &BTreeSet::from([valid.clone(), invalid]))
                .await;
        assert!(matches!(
            result,
            Err(IdentityDependencyError::InvalidSequence(0))
        ));
        assert!(require_association_state(&bo.context.db(), &valid).is_ok());
    }

    // verifies: PROC-012
    #[xmtp_common::test(unwrap_try = true)]
    fn a_failed_proof_blocks_before_an_absent_one() {
        let absent = || {
            Err(IdentityDependencyError::MissingReference(
                IdentityRequirement {
                    inbox_id: "inbox".into(),
                    sequence_id: 1,
                },
            ))
        };
        let failed = || {
            Err(IdentityDependencyError::Storage(
                StorageError::DbDeserialize,
            ))
        };
        assert!(parent_result([Ok(()), Ok(())]).is_ok());
        assert!(matches!(
            parent_result([Ok(()), absent()]),
            Err(IdentityDependencyError::MissingReference(_))
        ));
        for results in [[absent(), failed()], [failed(), absent()]] {
            assert!(matches!(
                parent_result(results),
                Err(IdentityDependencyError::Storage(_))
            ));
        }
    }

    // Absent references share one window that ends in a named absence after
    // successful queries, and another parent's reference does not queue behind it.
    #[xmtp_common::test(unwrap_try = true)]
    async fn absent_references_share_one_window() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let wait = Duration::from_secs(3);
        let mut settings = alix.context.incoming_runtime().policy().clone();
        settings.identity_reference_wait = wait;
        settings.max_dependency_requests = 1;
        let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
            .with_disable_workers(true)
            .stream_policy(settings)
            .build()
            .await?;
        let absent: BTreeSet<_> = (0..16)
            .map(|_| IdentityRequirement {
                inbox_id: hex::encode(xmtp_common::rand_vec::<32>()),
                sequence_id: 1,
            })
            .collect();
        let latest = bo.context.db().get_latest_sequence_id(&[bo.inbox_id()])?[bo.inbox_id()];
        let present = BTreeSet::from([IdentityRequirement {
            inbox_id: bo.inbox_id().to_owned(),
            sequence_id: latest as u64,
        }]);
        let queries = client.api_stats().query.get_count();
        let ((absent_result, absent_elapsed), (present_result, present_elapsed)) =
            futures::future::join(
                async {
                    let start = Instant::now();
                    let result = resolve_identity_requirements(&client.context, &absent).await;
                    (result, start.elapsed())
                },
                async {
                    sleep(Duration::from_millis(400)).await;
                    let start = Instant::now();
                    let result = resolve_identity_requirements(&client.context, &present).await;
                    (result, start.elapsed())
                },
            )
            .await;
        assert!(
            matches!(absent_result, Err(IdentityDependencyError::MissingReference(found)) if absent.contains(&found))
        );
        // Every round reloads the absent inboxes from the primary.
        assert!(client.api_stats().query.get_count() >= queries + 4);
        assert!(
            absent_elapsed >= wait && absent_elapsed < wait * 2,
            "absent references should share one window, took {absent_elapsed:?}"
        );
        assert!(present_result.is_ok());
        assert!(
            present_elapsed < wait / 2,
            "a present reference should not wait for a permit, took {present_elapsed:?}"
        );
    }
}
