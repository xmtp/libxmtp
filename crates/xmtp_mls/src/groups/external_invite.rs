//! Producer-side API for the external-invite ("QR-invite") flow.
//!
//! Any current member calls [`MlsGroup::create_external_invite`] to
//! produce two opaque byte blobs:
//!
//! 1. an [`ExternalInvitePayload`] proto wrapping a V1 variant carrying
//!    the application-supplied `service_pointer`, plus the
//!    `symmetric_key` and `external_group_id` read from the group's
//!    [`EXTERNAL_COMMIT_POLICY`] component; and
//! 2. an [`EncryptedGroupInfoBlob`] proto wrapping a TLS-serialized
//!    MLS `GroupInfo` for the current epoch (with the ratchet tree
//!    embedded) under that same symmetric key plus a fresh nonce,
//!    annotated with `epoch`, `group_state_hash`, and `expires_at_ns`.
//!
//! The symmetric key and the external group id live in the group (as
//! `EXTERNAL_COMMIT_POLICY.v1`), so they're stable across re-uploads
//! and across members: a just-joined external committer can re-export
//! and re-upload under the same key without re-issuing the QR.
//!
//! The application is responsible for transporting the payload (e.g.
//! via a QR code, deep link, or NFC tap) and for uploading the
//! encrypted blob to its own service indexed by the payload's
//! `external_group_id`. After every successful join, the joiner
//! re-exports and re-uploads under the same key with a fresh nonce.
//!
//! Crypto primitives live in `xmtp_mls_common`; this module only
//! orchestrates them.
//!
//! [`ExternalInvitePayload`]: xmtp_proto::xmtp::mls::message_contents::ExternalInvitePayload
//! [`EncryptedGroupInfoBlob`]: xmtp_proto::xmtp::mls::message_contents::EncryptedGroupInfoBlob
//! [`EXTERNAL_COMMIT_POLICY`]: xmtp_mls_common::app_data::component_id::ComponentId::EXTERNAL_COMMIT_POLICY

use openmls_traits::OpenMlsProvider as _;
use prost::Message as _;
use tls_codec::Serialize as _;

use crate::context::XmtpSharedContext;
use crate::groups::external_commit_policy::load_external_commit_policy;
use crate::groups::{GroupError, MlsGroup};
use xmtp_mls_common::invite::encrypted_group_info::wrap_group_info;
use xmtp_mls_common::invite::payload::{
    MIN_EXTERNAL_GROUP_ID_LEN, SYMMETRIC_KEY_LEN, build_payload,
};

/// Options for creating an external invite (QR-code or shareable-link join).
#[derive(Debug, Clone, Default)]
pub struct CreateExternalInviteOpts {
    /// Application-defined opaque bytes describing where the encrypted
    /// GroupInfo blob can be fetched (URL, service ID, etc.). Interpreted
    /// by the downstream consumer; libxmtp does not parse this.
    pub service_pointer: Vec<u8>,
    /// Optional caller-supplied tighter expiry hint (nanoseconds since
    /// UNIX epoch) for the encrypted blob. The effective expiry is the
    /// tightest of: this hint, the policy's absolute `expires_at_ns`,
    /// and `now_ns + policy.expire_in_ns`. `None` means "use whatever
    /// the policy says".
    pub blob_expires_at_ns: Option<u64>,
}

/// Output of [`MlsGroup::create_external_invite`]. Both fields are
/// protobuf-serialized bytes; the application chooses the transport
/// encoding for the invite payload and uploads the encrypted blob to
/// its service indexed by the payload's `external_group_id`.
#[derive(Debug, Clone)]
pub struct CreateExternalInviteOutput {
    /// Serialized [`ExternalInvitePayload`] proto (V1 envelope).
    ///
    /// [`ExternalInvitePayload`]: xmtp_proto::xmtp::mls::message_contents::ExternalInvitePayload
    pub invite_payload: Vec<u8>,
    /// Serialized [`EncryptedGroupInfoBlob`] proto (V1 envelope).
    ///
    /// [`EncryptedGroupInfoBlob`]: xmtp_proto::xmtp::mls::message_contents::EncryptedGroupInfoBlob
    pub encrypted_group_info: Vec<u8>,
}

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    /// Produce a QR-invite payload + an encrypted GroupInfo blob for the
    /// current epoch of this group.
    ///
    /// The symmetric key and `external_group_id` are taken from
    /// [`EXTERNAL_COMMIT_POLICY.v1`] (populated by
    /// [`MlsGroup::set_allow_external_commit`] when enabled). Calling this
    /// before an admin has enabled the policy fails with
    /// [`GroupError::ExternalCommitNotAllowed`].
    ///
    /// The exported `GroupInfo` always carries the ratchet tree (i.e.
    /// `with_ratchet_tree = true`) so that the joining client can perform
    /// an external commit without an additional out-of-band lookup. The
    /// blob carries `epoch` and `group_state_hash` (epoch authenticator
    /// bytes) so the service can total-order uploads and reject
    /// same-epoch fork uploads.
    ///
    /// The blob's `expires_at_ns` is derived from the policy's
    /// `expires_at_ns` (absolute) and `expire_in_ns` (relative to
    /// upload time), with an optional per-call tightening via
    /// [`CreateExternalInviteOpts::blob_expires_at_ns`].
    ///
    /// [`EXTERNAL_COMMIT_POLICY.v1`]: xmtp_proto::xmtp::mls::message_contents::ExternalCommitPolicyV1
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn create_external_invite(
        &self,
        opts: CreateExternalInviteOpts,
    ) -> Result<CreateExternalInviteOutput, GroupError> {
        self.ensure_not_paused().await?;

        let signer = self.context.identity().installation_keys.clone();

        self.load_mls_group_with_lock_async(async |openmls_group| {
            // Read EXTERNAL_COMMIT_POLICY. Any member with the policy
            // visible in their AppData dict can produce an invite from
            // the in-group key + external_group_id.
            let policy = load_external_commit_policy(&openmls_group)?
                .ok_or(GroupError::ExternalCommitNotAllowed)?;

            if !policy.allow_external_commit {
                return Err(GroupError::ExternalCommitNotAllowed);
            }
            if policy.symmetric_key.len() != SYMMETRIC_KEY_LEN {
                return Err(GroupError::ExternalCommitPolicyMalformed(format!(
                    "symmetric_key has wrong length: got {}, expected {}",
                    policy.symmetric_key.len(),
                    SYMMETRIC_KEY_LEN,
                )));
            }
            if policy.external_group_id.len() < MIN_EXTERNAL_GROUP_ID_LEN {
                return Err(GroupError::ExternalCommitPolicyMalformed(format!(
                    "external_group_id too short: got {}, min {}",
                    policy.external_group_id.len(),
                    MIN_EXTERNAL_GROUP_ID_LEN,
                )));
            }

            let key_array: [u8; SYMMETRIC_KEY_LEN] = policy
                .symmetric_key
                .as_slice()
                .try_into()
                .expect("length checked immediately above");

            let provider = self.context.mls_provider();
            let group_info_message =
                openmls_group.export_group_info(provider.crypto(), &signer, true)?;
            let group_info_bytes = group_info_message.tls_serialize_detached()?;

            let epoch = openmls_group.epoch().as_u64();
            let group_state_hash = openmls_group.epoch_authenticator().as_slice().to_vec();
            let blob_expires_at_ns = compute_blob_expiry(
                &policy,
                opts.blob_expires_at_ns,
                xmtp_common::time::now_ns(),
            );

            let encrypted_blob = wrap_group_info(
                &group_info_bytes,
                &key_array,
                epoch,
                group_state_hash,
                blob_expires_at_ns,
            )?;

            let payload = build_payload(
                opts.service_pointer,
                policy.external_group_id.clone(),
                key_array,
            );

            Ok::<CreateExternalInviteOutput, GroupError>(CreateExternalInviteOutput {
                invite_payload: payload.encode_to_vec(),
                encrypted_group_info: encrypted_blob.encode_to_vec(),
            })
        })
        .await
    }
}

/// Derive the blob's effective `expires_at_ns` from policy + per-call hint.
///
/// Tightest active expiry wins. `0` anywhere means "this rule contributes
/// nothing"; absent contributors produce a `0` (no expiry) blob.
fn compute_blob_expiry(
    policy: &xmtp_proto::xmtp::mls::message_contents::ExternalCommitPolicyV1,
    opts_expires_at_ns: Option<u64>,
    now_ns: i64,
) -> u64 {
    let mut candidates: Vec<u64> = Vec::new();
    if policy.expires_at_ns != 0 {
        candidates.push(policy.expires_at_ns);
    }
    if policy.expire_in_ns != 0 {
        let now_u64 = u64::try_from(now_ns).unwrap_or(0);
        candidates.push(now_u64.saturating_add(policy.expire_in_ns));
    }
    if let Some(t) = opts_expires_at_ns
        && t != 0
    {
        candidates.push(t);
    }
    candidates.into_iter().min().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    //! Unit-level coverage for the producer path. End-to-end coverage
    //! that exercises the full
    //! `set_allow_external_commit → create_external_invite → join`
    //! pipeline (including blob round-trip + epoch + state hash
    //! assertions) lives in the QR-invite integration test (T-1).
    use super::*;
    use crate::tester;
    use xmtp_mls_common::invite::payload::{MIN_EXTERNAL_GROUP_ID_LEN, SYMMETRIC_KEY_LEN};
    use xmtp_proto::xmtp::mls::message_contents::ExternalCommitPolicyV1;

    #[xmtp_common::test(unwrap_try = true)]
    async fn create_external_invite_fails_when_policy_not_set() {
        tester!(alix);
        let group = alix.create_group(None, None)?;

        let err = group
            .create_external_invite(CreateExternalInviteOpts::default())
            .await
            .unwrap_err();
        assert!(
            matches!(err, GroupError::ExternalCommitNotAllowed),
            "expected ExternalCommitNotAllowed, got {err:?}",
        );
    }

    // NOTE: end-to-end tests that exercise
    //   set_allow_external_commit(true|false) → create_external_invite
    // live in the QR-invite integration test (T-1, libxmtp-integration-test).
    // The L-6 setter routes through the AppDataUpdate intent path; the full
    // commit / decrypt / round-trip surface is best validated in a setting
    // that also exercises the joiner.

    #[test]
    fn compute_blob_expiry_picks_tightest_active_value() {
        // Policy with absolute expiry, no relative window, no opts hint.
        let policy = ExternalCommitPolicyV1 {
            allow_external_commit: true,
            expires_at_ns: 5_000,
            expire_in_ns: 0,
            symmetric_key: vec![0u8; SYMMETRIC_KEY_LEN],
            external_group_id: vec![0u8; MIN_EXTERNAL_GROUP_ID_LEN],
        };
        assert_eq!(compute_blob_expiry(&policy, None, 1_000), 5_000);

        // Policy with relative window picks up `now + expire_in_ns`.
        let policy = ExternalCommitPolicyV1 {
            expire_in_ns: 200,
            ..Default::default()
        };
        assert_eq!(compute_blob_expiry(&policy, None, 1_000), 1_200);

        // Opts hint tighter than policy wins.
        let policy = ExternalCommitPolicyV1 {
            expires_at_ns: 10_000,
            ..Default::default()
        };
        assert_eq!(compute_blob_expiry(&policy, Some(3_000), 0), 3_000);

        // Opts hint zero is ignored (treated as "unset").
        let policy = ExternalCommitPolicyV1 {
            expires_at_ns: 7_000,
            ..Default::default()
        };
        assert_eq!(compute_blob_expiry(&policy, Some(0), 0), 7_000);

        // No active expiry anywhere → 0 (no expiry).
        let policy = ExternalCommitPolicyV1::default();
        assert_eq!(compute_blob_expiry(&policy, None, 1_000), 0);
    }
}
