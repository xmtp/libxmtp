//! Reusable key-package maintenance helpers (delete expired / rotate / next deadline).
//!
//! This module lifts the delete + rotate logic out of the polling
//! [`KeyPackagesCleanerWorker`](super::key_package_cleaner) so it can be driven by the
//! TaskRunner instead. The old worker still exists in this tree; it is removed in a later
//! task. Both temporarily contain similar logic by design.

use crate::context::XmtpSharedContext;
use crate::identity::IdentityError;
use crate::identity::pq_key_package_references_key;
use crate::worker::key_package_cleaner::KeyPackagesCleanerError;
use openmls_traits::storage::StorageProvider;
use xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION;
use xmtp_db::prelude::*;
use xmtp_db::{
    MlsProviderExt,
    sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY},
};

/// Soonest of the rotation/deletion deadlines, or `now + NS_IN_DAY` if neither is set.
pub(crate) fn next_deadline_from(rotation: Option<i64>, delete: Option<i64>, now: i64) -> i64 {
    [rotation, delete]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(now + xmtp_common::NS_IN_DAY)
}

/// Reusable, context-driven key-package maintenance operations.
///
/// These are the delete/rotate operations lifted from the polling
/// `KeyPackagesCleanerWorker` so the TaskRunner can call them directly via
/// `KeyPackageMaintenance::delete_expired(context)` etc.
pub(crate) struct KeyPackageMaintenance;

impl KeyPackageMaintenance {
    /// Delete a single key package (and its PQ references) from the local key store.
    fn delete_key_package<Context>(
        context: &Context,
        hash_ref: Vec<u8>,
        pq_pub_key: Option<Vec<u8>>,
    ) -> Result<(), IdentityError>
    where
        Context: XmtpSharedContext,
    {
        let openmls_hash_ref = crate::identity::deserialize_key_package_hash_ref(&hash_ref)?;
        let mls_provider = context.mls_provider();
        let key_store = mls_provider.key_store();

        key_store.delete_key_package(&openmls_hash_ref)?;

        if let Some(pq_pub_key) = pq_pub_key {
            key_store.delete(
                KEY_PACKAGE_REFERENCES,
                pq_key_package_references_key(&pq_pub_key)?.as_slice(),
            )?;
            key_store.delete(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, &hash_ref)?;
        }

        Ok(())
    }

    /// Delete all expired key packages from the local DB and key store.
    pub(crate) fn delete_expired<Context>(context: &Context) -> Result<(), KeyPackagesCleanerError>
    where
        Context: XmtpSharedContext,
    {
        let conn = context.db();

        // Propagate (don't swallow): a swallowed fetch error never triggered the supervisor's
        // reconnect path, so a pool outage retried silently every 5s.
        let expired_kps = conn
            .get_expired_key_packages()
            .map_err(KeyPackagesCleanerError::Fetch)?;
        if expired_kps.is_empty() {
            return Ok(());
        }

        tracing::info!("Deleting {} expired key packages", expired_kps.len());
        // Delete from local db
        for kp in &expired_kps {
            Self::delete_key_package(
                context,
                kp.key_package_hash_ref.clone(),
                kp.post_quantum_public_key.clone(),
            )
            .map_err(KeyPackagesCleanerError::DeleteKeyPackage)?;
        }

        // Delete from database using the max expired ID
        if let Some(max_id) = expired_kps.iter().map(|kp| kp.id).max() {
            conn.delete_key_package_history_up_to_id(max_id)
                .map_err(KeyPackagesCleanerError::Deletion)?;
            tracing::info!(
                "Deleted {} expired key packages (up to ID {}) from local DB and state",
                expired_kps.len(),
                max_id
            );
        }

        Ok(())
    }

    /// Rotate and upload a new key package if the identity is due for rotation.
    pub(crate) async fn rotate_if_needed<Context>(
        context: &Context,
    ) -> Result<(), KeyPackagesCleanerError>
    where
        Context: XmtpSharedContext,
    {
        let conn = context.db();

        if conn
            .is_identity_needs_rotation()
            .map_err(KeyPackagesCleanerError::Metadata)?
        {
            tracing::info!("Rotating key package");
            context
                .identity()
                .rotate_and_upload_key_package(
                    context.api(),
                    context.mls_storage(),
                    CREATE_PQ_KEY_PACKAGE_EXTENSION,
                )
                .await
                .map_err(KeyPackagesCleanerError::Rotation)?;
            tracing::info!("Key package rotation successful");
        }

        Ok(())
    }

    /// Soonest of the next rotation / pending-deletion deadlines, or `now + NS_IN_DAY`.
    pub(crate) fn next_deadline<Context>(context: &Context) -> Result<i64, KeyPackagesCleanerError>
    where
        Context: XmtpSharedContext,
    {
        let conn = context.db();
        let rotation = conn
            .next_key_package_rotation_ns()
            .map_err(KeyPackagesCleanerError::Metadata)?;
        let delete = conn
            .min_key_package_delete_at_ns()
            .map_err(KeyPackagesCleanerError::Metadata)?;
        Ok(next_deadline_from(
            rotation,
            delete,
            xmtp_common::time::now_ns(),
        ))
    }
}

#[cfg(test)]
mod tests {
    #[xmtp_common::test]
    fn next_deadline_picks_soonest_or_falls_back() {
        use super::next_deadline_from;
        let now = 1_000i64;
        let day = xmtp_common::NS_IN_DAY;
        assert_eq!(next_deadline_from(None, None, now), now + day); // both absent -> fallback
        assert_eq!(next_deadline_from(Some(now + 50), None, now), now + 50); // rotation only
        assert_eq!(
            next_deadline_from(Some(now + 50), Some(now + 20), now),
            now + 20
        ); // delete sooner
        assert_eq!(next_deadline_from(None, Some(now + 20), now), now + 20); // delete only
    }

    /// Requires a live XMTP node. End-to-end behavior guard for the TaskRunner-driven
    /// KP maintenance: `client.queue_key_rotation()` marks the identity for rotation
    /// AND nudges the recurring KpMaintenance task (PR #2044 ~5s debounce). The
    /// TaskRunner then wakes, rotates, and uploads a fresh key package — so the
    /// on-network init key must change within the poll window.
    ///
    /// WASM-SAFE: the poll is iteration-counted with `xmtp_common::time::sleep`,
    /// never `std::time::Instant` (which panics on wasm).
    #[xmtp_common::test(unwrap_try = true)]
    async fn queue_key_rotation_wakes_taskrunner_and_rotates() {
        use crate::tester;

        // Default `tester!` builds with workers (the TaskRunner) enabled — required
        // so the queued rotation is actually driven to completion.
        tester!(client);

        let installation_id = client.installation_public_key().to_vec();

        // Snapshot the current on-network init key.
        // `VerifiedKeyPackageV2::hpke_init_key()` returns an owned Vec<u8>.
        let before = {
            let mut map = client
                .get_key_packages_for_installation_ids(vec![installation_id.clone()])
                .await?;
            let entry = map
                .remove(&installation_id)
                .expect("installation not found in response")?;
            entry.hpke_init_key()
        };

        // Queue a rotation: marks the identity AND nudges the KpMaintenance task.
        client.queue_key_rotation().await?;

        // Poll up to 12 s (60 × 200 ms) for the on-network key to change.
        // Uses `xmtp_common::time::sleep` so the test compiles for wasm too
        // (std::time::Instant panics on wasm).
        let mut after = None;
        for _ in 0..60u32 {
            xmtp_common::time::sleep(std::time::Duration::from_millis(200)).await;
            let mut map = client
                .get_key_packages_for_installation_ids(vec![installation_id.clone()])
                .await?;
            let entry = map
                .remove(&installation_id)
                .expect("installation not found in response")?;
            let key = entry.hpke_init_key();
            if key != before {
                after = Some(key);
                break;
            }
        }

        assert!(
            after.is_some(),
            "key package did not rotate after queue_key_rotation + TaskRunner wake"
        );
    }
}
