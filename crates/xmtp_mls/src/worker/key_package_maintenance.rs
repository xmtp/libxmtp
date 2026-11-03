//! Key-package maintenance as TaskRunner consumers: payload/seed helpers and the
//! rotate/sweep work the `KpRotation`/`KpDeletion` dispatch arms call into.
//! Recurrence + nudging come from the generic layer (TaskOutcome, PullInDeadline).

use crate::identity::IdentityError;
use crate::identity_updates::IdentityContext;
use crate::state_tx::state_write;
use crate::worker::NeedsDbReconnect;
use crate::worker::tasks::enqueue_pull_in;
use thiserror::Error;
use tls_codec::Serialize;
use xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION;
use xmtp_db::StorageError;
use xmtp_db::TransactionOutcome::Continue;
use xmtp_db::prelude::*;
use xmtp_db::sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY};
use xmtp_db::tasks::{NEVER_EXPIRES, NewTask, TaskDataHash, data_hash_for};
use xmtp_proto::xmtp::mls::database::{
    KpDeletion, KpRotation, Task as TaskProto, task::Task as TaskKind,
};

#[derive(Debug, Error)]
pub enum KeyPackageMaintenanceError {
    #[error("generic storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("generic identity error: {0}")]
    Identity(#[from] IdentityError),
    #[error("metadata error: {0}")]
    Metadata(StorageError),
    #[error("failed to fetch expired key packages: {0}")]
    Fetch(StorageError),
    #[error("failed to delete key package: {0}")]
    DeleteKeyPackage(IdentityError),
    #[error("deletion error: {0}")]
    Deletion(StorageError),
    #[error("rotation error: {0}")]
    Rotation(IdentityError),
}

impl xmtp_common::RetryableError for KeyPackageMaintenanceError {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn is_retryable(&self) -> bool {
        match self {
            Self::Identity(e) | Self::DeleteKeyPackage(e) | Self::Rotation(e) => e.is_retryable(),
            Self::Storage(e) | Self::Metadata(e) | Self::Fetch(e) | Self::Deletion(e) => {
                e.is_retryable()
            }
        }
    }

    /// The delay of a rate-limit rejection that failed this maintenance step.
    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        match self {
            Self::Identity(e) | Self::DeleteKeyPackage(e) | Self::Rotation(e) => {
                e.rate_limit_delay()
            }
            Self::Storage(_) | Self::Metadata(_) | Self::Fetch(_) | Self::Deletion(_) => None,
        }
    }
}

impl NeedsDbReconnect for KeyPackageMaintenanceError {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Identity(s) => s.needs_db_reconnect(),
            Self::Metadata(s) => s.db_needs_connection(),
            Self::Fetch(s) => s.db_needs_connection(),
            Self::DeleteKeyPackage(s) => s.needs_db_reconnect(),
            Self::Deletion(s) => s.db_needs_connection(),
            Self::Rotation(s) => s.needs_db_reconnect(),
        }
    }
}

pub(crate) fn kp_rotation_proto() -> TaskProto {
    TaskProto {
        task: Some(TaskKind::KpRotation(KpRotation {})),
    }
}

pub(crate) fn kp_deletion_proto() -> TaskProto {
    TaskProto {
        task: Some(TaskKind::KpDeletion(KpDeletion {})),
    }
}

pub(crate) fn kp_rotation_hash() -> TaskDataHash {
    data_hash_for(&kp_rotation_proto())
}

pub(crate) fn kp_deletion_hash() -> TaskDataHash {
    data_hash_for(&kp_deletion_proto())
}

/// Never-expire recurring seed: the reaper's
/// `expires_at_ns < now || attempts >= max_attempts` check can never fire.
pub(crate) fn kp_seed(proto: TaskProto, now: i64) -> Result<NewTask, StorageError> {
    NewTask::builder()
        .originating_message_sequence_id(0)
        .expires_at_ns(NEVER_EXPIRES)
        .max_attempts(i32::MAX)
        .next_attempt_at_ns(now)
        .build(proto)
}

/// Upload a fresh key package when rotation is due and return whether it occurred.
/// A confirmed receipt starts retirement of keys published earlier by the backend.
pub(crate) async fn rotate_if_needed<Context: IdentityContext>(
    context: &Context,
) -> Result<bool, KeyPackageMaintenanceError> {
    if !context
        .db()
        .is_identity_needs_rotation()
        .map_err(KeyPackageMaintenanceError::Metadata)?
    {
        return Ok(false);
    }
    crate::identity_lifecycle::rotate_and_upload_key_package(
        context.identity(),
        context.api(),
        context.mls_storage(),
        CREATE_PQ_KEY_PACKAGE_EXTENSION,
    )
    .await
    .map_err(KeyPackageMaintenanceError::Rotation)?;
    Ok(true)
}

/// Delete one key package's local material (keystore entry + PQ references).
#[cfg(any(test, feature = "test-utils"))]
pub fn delete_key_package<Context: IdentityContext>(
    context: &Context,
    hash_ref: Vec<u8>,
    pq_pub_key: Option<Vec<u8>>,
) -> Result<(), IdentityError> {
    state_write(context.mls_storage(), |tx| {
        delete_key_package_material(&tx.storage(), &hash_ref, pq_pub_key.as_deref())?;
        Ok::<_, IdentityError>(Continue(()))
    })?;
    Ok(())
}

/// Delete all parts of one key package under the current database writer.
fn delete_key_package_material(
    key_store: &impl XmtpMlsStorageProvider,
    hash_ref: &[u8],
    pq_pub_key: Option<&[u8]>,
) -> Result<(), IdentityError> {
    let openmls_hash_ref = crate::identity::deserialize_key_package_hash_ref(hash_ref)?;
    let bundle: Option<openmls::prelude::KeyPackageBundle> =
        key_store.key_package(&openmls_hash_ref)?;
    if let Some(bundle) = bundle {
        let init_key = bundle
            .key_package()
            .hpke_init_key()
            .tls_serialize_detached()?;
        key_store.delete(KEY_PACKAGE_REFERENCES, &init_key)?;
    }
    key_store.delete_key_package(&openmls_hash_ref)?;

    if let Some(pq_pub_key) = pq_pub_key {
        key_store.delete(
            KEY_PACKAGE_REFERENCES,
            crate::identity::pq_key_package_references_key(pq_pub_key)?.as_slice(),
        )?;
        key_store.delete(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, hash_ref)?;
    }

    Ok(())
}

/// Delete a refused key package's material and history in one transaction.
// implements: JOIN-020
pub(crate) fn discard_key_package(
    mls_storage: &impl XmtpMlsStorageProvider,
    history_id: i32,
) -> Result<(), IdentityError> {
    state_write(mls_storage, |tx| {
        let storage = tx.storage();
        let history = storage
            .db()
            .find_key_package_history_entries_before_id(i32::MAX)?;
        for kp in history.iter().filter(|kp| kp.id == history_id) {
            let pq_pub_key = kp.post_quantum_public_key.as_deref();
            delete_key_package_material(&storage, &kp.key_package_hash_ref, pq_pub_key)?;
            storage.db().delete_key_package_entry_with_id(kp.id)?;
        }
        Ok::<_, IdentityError>(Continue(()))
    })?;
    Ok(())
}

/// Keep expired keys while any durable Welcome remains pending.
/// Otherwise, delete each expired key's material and history in one transaction.
pub(crate) fn sweep_expired<Context: IdentityContext>(
    context: &Context,
) -> Result<(), KeyPackageMaintenanceError> {
    state_write(context.mls_storage(), |tx| {
        let storage = tx.storage();
        let conn = storage.db();
        // The durable Welcome queue owns the key-retention obligation.
        if conn.has_pending_welcomes()? {
            return Ok::<_, IdentityError>(Continue(()));
        }
        let expired = conn.get_expired_key_packages()?;
        for kp in &expired {
            delete_key_package_material(
                &storage,
                &kp.key_package_hash_ref,
                kp.post_quantum_public_key.as_deref(),
            )?;
            conn.delete_key_package_entry_with_id(kp.id)?;
        }
        Ok(Continue(()))
    })?;
    Ok(())
}

/// Queue rotation within five seconds and persist its task in one transaction.
/// Create the recurring task before its deadline update, then emit after commit.
pub(crate) fn queue_key_rotation<Context: IdentityContext>(
    context: &Context,
) -> Result<(), StorageError> {
    crate::state_tx::state_write_with_events(
        context.mls_storage(),
        context.events(),
        |tx, events| {
            queue_key_rotation_in(&tx.storage())?;
            xmtp_events::EventWriter::emit(
                events,
                None,
                Some(crate::subscriptions::internal::InternalEvent::TaskScheduled),
            );
            Ok::<_, StorageError>(Continue(()))
        },
    )?;
    Ok(())
}

/// Queue rotation on the caller's writer so a Welcome install can commit with it.
/// The caller emits the task fact in its event buffer.
pub(crate) fn queue_key_rotation_in(
    storage: &impl XmtpMlsStorageProvider,
) -> Result<(), StorageError> {
    let now = xmtp_common::time::now_ns();
    storage
        .db()
        .queue_key_rotation_with_nudge(&kp_rotation_hash(), kp_seed(kp_rotation_proto(), now)?)
}

/// After anything marks superseded KPs for deletion: ensure the KpDeletion
/// singleton exists (pull-in against a missing target is a no-op), then pull
/// it in to the earliest pending delete_at. No-op when nothing is marked, so
/// it is safe (and idempotent) to call on every dispatch.
pub(crate) fn nudge_deletion<Context: IdentityContext>(
    context: &Context,
) -> Result<(), StorageError> {
    let db = context.db();
    let now = xmtp_common::time::now_ns();
    if let Some(at) = db.min_key_package_delete_at_ns()? {
        db.create_or_ignore_task(kp_seed(kp_deletion_proto(), now)?)?;
        enqueue_pull_in(context, kp_deletion_hash(), at, NEVER_EXPIRES)?;
    }
    Ok(())
}

/// Idempotent startup seeding + reconcile: pull-ins only LOWER task deadlines to
/// the live DB columns, repairing rows stranded by a crash mid-nudge.
pub(crate) fn seed_and_reconcile_kp_tasks<Context: IdentityContext>(
    context: &Context,
) -> Result<(), StorageError> {
    let db = context.db();
    let now = xmtp_common::time::now_ns();
    db.create_or_ignore_task(kp_seed(kp_rotation_proto(), now)?)?;
    db.create_or_ignore_task(kp_seed(kp_deletion_proto(), now)?)?;
    // None = pre-registration (no identity row): the seed row already fires at
    // startup; a pull-in to `now` would be redundant noise.
    if let Some(rot) = db.next_key_package_rotation_ns()? {
        enqueue_pull_in(context, kp_rotation_hash(), rot, NEVER_EXPIRES)?;
    }
    if let Some(del) = db.min_key_package_delete_at_ns()? {
        enqueue_pull_in(context, kp_deletion_hash(), del, NEVER_EXPIRES)?;
    }
    Ok(())
}

// These tests use the native connection pool and its error variants.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::identity_lifecycle::test_support::IdentityTestPeer;
    use crate::prelude::*;
    use openmls_traits::storage::StorageProvider;
    use prost::Message;
    use xmtp_db::ConnectionExt;
    use xmtp_proto::xmtp::mls::database::Task as TaskProtoDecode;

    fn row_by_hash(db: &impl QueryTasks, hash: impl AsRef<[u8]>) -> Option<xmtp_db::tasks::Task> {
        db.get_tasks()
            .expect("get_tasks should not fail")
            .into_iter()
            .find(|t| t.data_hash == hash.as_ref())
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn pending_welcome_preserves_expired_keys_until_completion() {
        use xmtp_db::diesel::prelude::*;
        use xmtp_db::incoming_envelope::{
            IncomingLimits, NetworkEntityKind, NewIncomingEnvelope, PendingBudget, StreamTopic,
        };
        use xmtp_db::schema::key_package_history::dsl;
        use xmtp_proto::types::Cursor;

        let alix = IdentityTestPeer::new().await;
        let db = alix.context.db();
        let history = db
            .find_key_package_history_entries_before_id(i32::MAX)?
            .pop()?;
        let hash =
            crate::identity::deserialize_key_package_hash_ref(&history.key_package_hash_ref)?;
        db.raw_query(|conn| {
            diesel::update(dsl::key_package_history.filter(dsl::id.eq(history.id)))
                .set(dsl::delete_at_ns.eq(0))
                .execute(conn)
        })?;
        let topic = StreamTopic {
            entity_id: alix.context.installation_id().to_vec(),
            kind: NetworkEntityKind::Welcome,
        };
        let limit = PendingBudget { rows: 1, bytes: 1 };
        db.admit_ordered_batch(
            &topic,
            Cursor(0),
            &[NewIncomingEnvelope {
                sequence_id: Cursor(1),
                envelope: vec![1],
            }],
            IncomingLimits {
                batch: limit,
                topic: limit,
                kind: limit,
            },
        )?;

        sweep_expired(&alix.context)?;
        let retained: Option<openmls::prelude::KeyPackageBundle> =
            alix.context.mls_storage().key_package(&hash)?;
        assert!(retained.is_some());

        db.complete_pending_envelope(&topic, Cursor(1))?;
        sweep_expired(&alix.context)?;
        let removed: Option<openmls::prelude::KeyPackageBundle> =
            alix.context.mls_storage().key_package(&hash)?;
        assert!(removed.is_none());
        assert!(
            db.find_key_package_history_entry_by_hash_ref(history.key_package_hash_ref)
                .is_err()
        );
    }

    /// The welcome nudge must self-heal a missing rotation seed (e.g. startup
    /// seeding never ran) instead of enqueuing a dropped-on-miss pull-in.
    #[xmtp_common::test(unwrap_try = true)]
    async fn welcome_nudge_selfheals_missing_rotation_seed() {
        let alix = IdentityTestPeer::new().await; // no TaskRunner -> no seeds
        let db = alix.context.db();
        assert!(row_by_hash(&db, kp_rotation_hash()).is_none());

        queue_key_rotation(&alix.context)?;

        assert!(
            row_by_hash(&db, kp_rotation_hash()).is_some(),
            "nudge must recreate the missing KpRotation singleton"
        );
        let has_pull_in = db.get_tasks()?.iter().any(|t| {
            matches!(
                TaskProtoDecode::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                Some(TaskKind::PullInDeadline(p)) if p.target_data_hash == kp_rotation_hash().as_ref()
            )
        });
        assert!(has_pull_in, "nudge must enqueue a rotation pull-in");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn queued_rotation_is_visible_before_its_task_fact() {
        use crate::context::CoreContext;
        use crate::core::events::InternalEvent;
        use xmtp_events::EventFilter;

        let alix = IdentityTestPeer::new().await;
        let subscription = alix.context.events().subscribe(
            EventFilter::default()
                .with_internal(|event| matches!(event, InternalEvent::TaskScheduled)),
            Some(10),
        );
        let before = alix.context.events().subscribe(
            EventFilter::default()
                .with_internal(|event| matches!(event, InternalEvent::TaskScheduled)),
            Some(10),
        );
        let fired = std::rc::Rc::new(std::cell::Cell::new(0));
        let hook = crate::core::precommit_test_hook::install({
            let fired = fired.clone();
            move |_| {
                fired.set(fired.get() + 1);
                assert!(before.drain().is_empty());
            }
        });
        queue_key_rotation(&alix.context)?;
        drop(hook);
        assert!(fired.get() > 0);
        assert!(row_by_hash(&alix.context.db(), kp_rotation_hash()).is_some());
        assert!(matches!(
            subscription.next().await.unwrap().internal,
            Some(InternalEvent::TaskScheduled)
        ));
    }
}

#[cfg(test)]
mod deletion_tests;

// These fixtures use native pool errors. Browser connection lifecycle tests
// cover closed persistent handles in the WASM database module.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod disconnect_propagation_tests {
    //! Pins that a dropped-pool signal survives this owner's error types, so
    //! `needs_db_reconnect()` stays `true` for the worker that surfaces them.
    use super::KeyPackageMaintenanceError;
    use crate::core::reconnect::NeedsDbReconnect;
    use xmtp_db::{ConnectionError, PlatformStorageError, StorageError};

    /// A `StorageError` that signals the connection pool was dropped.
    fn disconnect_storage() -> StorageError {
        StorageError::Platform(PlatformStorageError::PoolNeedsConnection)
    }

    /// A `ConnectionError` that signals the connection pool was dropped.
    fn disconnect_connection() -> ConnectionError {
        ConnectionError::Platform(PlatformStorageError::PoolNeedsConnection)
    }

    /// A storage error that is NOT a disconnect — must never trip the contract.
    fn benign_storage() -> StorageError {
        StorageError::InvalidHmacLength
    }

    #[xmtp_common::test]
    fn key_package_maintenance_error_forwards_disconnect() {
        use crate::identity::IdentityError;
        assert!(KeyPackageMaintenanceError::Storage(disconnect_storage()).needs_db_reconnect());
        // Per-key-package delete returns an IdentityError; a disconnect must
        // bubble whether it arrives as a StorageError or a bare ConnectionError.
        assert!(
            KeyPackageMaintenanceError::Identity(IdentityError::StorageError(disconnect_storage()))
                .needs_db_reconnect()
        );
        assert!(
            KeyPackageMaintenanceError::Identity(IdentityError::Db(disconnect_connection()))
                .needs_db_reconnect()
        );
        assert!(!KeyPackageMaintenanceError::Storage(benign_storage()).needs_db_reconnect());
    }
}
