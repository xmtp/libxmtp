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
use crate::groups::external_commit_policy::{load_external_commit_policy, validate_policy_v1};
use crate::groups::{GroupError, MlsGroup};
use xmtp_db::prelude::*;
use xmtp_mls_common::invite::encrypted_group_info::{effective_expires_at_ns, wrap_group_info};
use xmtp_mls_common::invite::payload::{SYMMETRIC_KEY_LEN, build_payload};
use xmtp_proto::xmtp::mls::message_contents::ServicePointer;

/// Options for creating an external invite (QR-code or shareable-link join).
#[derive(Debug, Clone, Default)]
pub struct CreateExternalInviteOpts {
    /// Where the encrypted GroupInfo blob can be fetched: an `https_url`
    /// or application-interpreted `opaque` bytes. `None` means
    /// application-resolved — the consuming app already knows how to
    /// reach the service for its own invites and saves the QR space.
    pub service_pointer: Option<ServicePointer>,
    /// Optional caller-supplied tighter expiry hint (nanoseconds since
    /// UNIX epoch) for the encrypted blob. The effective expiry is the
    /// tightest of: this hint, the policy's absolute `expires_at_ns`,
    /// and `epoch_start + policy.expire_in_ns` (the staleness window
    /// validators enforce — folding it in here means a joiner's single
    /// expiry check also skips candidates validators would reject).
    /// `None` means "use whatever the policy says".
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
    /// [`EXTERNAL_COMMIT_POLICY.v1`] (minted by
    /// [`MlsGroup::enable_external_commits`]). Calling this before an
    /// admin has enabled the policy fails with
    /// [`GroupError::ExternalCommitNotAllowed`]; a policy that violates
    /// the XIP-82 lifecycle invariants surfaces the structured
    /// [`GroupError::ExternalCommitPolicy`] error.
    ///
    /// The exported `GroupInfo` always carries the ratchet tree (i.e.
    /// `with_ratchet_tree = true`) so that the joining client can perform
    /// an external commit without an additional out-of-band lookup. The
    /// blob carries `epoch` and `group_state_hash` (epoch authenticator
    /// bytes), both authenticated as AAD of the AEAD wrap — the joiner
    /// validates-and-selects across the service's candidate set, the
    /// service never arbitrates.
    ///
    /// The blob's `expires_at_ns` is the *effective* expiry: the
    /// tightest of the policy's absolute `expires_at_ns` and
    /// `epoch_start + expire_in_ns` (the staleness bound validators
    /// enforce against envelope timestamps — folded in here so the
    /// joiner's single expiry check also skips candidates validators
    /// would reject as stale), optionally tightened per call via
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
            // Epoch start (envelope time) for the effective-expiry fold
            // — the same value the validator's check 9 measures from.
            // Read under the group lock so a concurrent commit merge
            // can't advance the epoch between this read and the
            // GroupInfo export below. NULL (no epoch advance since the
            // column landed) falls back to group creation, the initial
            // epoch.
            let epoch_started_at_ns = self
                .context
                .db()
                .find_group(&self.group_id)?
                .map(|group| group.epoch_entered_at_ns.unwrap_or(group.created_at_ns))
                .unwrap_or(0)
                .max(0) as u64;
            // Read EXTERNAL_COMMIT_POLICY. Any current member with the
            // policy visible in their AppData dict can produce an
            // invite from the in-group key + external_group_id.
            let policy = load_external_commit_policy(&openmls_group)?
                .ok_or(GroupError::ExternalCommitNotAllowed)?;
            if !policy.allow_external_commit {
                return Err(GroupError::ExternalCommitNotAllowed);
            }
            // Enabled ⇒ 32-byte key + ≥4-byte slot id + well-formed
            // refresh pointers (the same invariants every validator
            // enforced when this policy was written).
            validate_policy_v1(&policy)?;

            let key_array: [u8; SYMMETRIC_KEY_LEN] = policy
                .symmetric_key
                .as_ref()
                .expect("validate_policy_v1 guarantees presence when enabled")
                .material
                .as_slice()
                .try_into()
                .expect("validate_policy_v1 guarantees length");

            let provider = self.context.mls_provider();
            let group_info_message =
                openmls_group.export_group_info(provider.crypto(), &signer, true)?;
            let group_info_bytes = group_info_message.tls_serialize_detached()?;

            let epoch = openmls_group.epoch().as_u64();
            let group_state_hash = openmls_group.epoch_authenticator().as_slice().to_vec();

            // Effective expiry per the spec'd fold; a per-call hint can
            // only tighten it further.
            let effective = effective_expires_at_ns(
                policy.expires_at_ns,
                epoch_started_at_ns,
                policy.expire_in_ns,
            );
            let blob_expires_at_ns = tighten_expiry(effective, opts.blob_expires_at_ns);

            // Fresh CSPRNG nonce per wrap (builder default): re-wraps
            // under the same key MUST never share a nonce.
            let encrypted_blob = wrap_group_info()
                .plaintext(&group_info_bytes)
                .key(&key_array)
                .epoch(epoch)
                .group_state_hash(group_state_hash)
                .expires_at_ns(blob_expires_at_ns)
                .call()?;

            let payload = build_payload(
                opts.service_pointer,
                policy.external_group_id.clone(),
                key_array,
            )?;

            Ok::<CreateExternalInviteOutput, GroupError>(CreateExternalInviteOutput {
                invite_payload: payload.encode_to_vec(),
                encrypted_group_info: encrypted_blob.encode_to_vec(),
            })
        })
        .await
    }
}

/// Tightest of the policy-derived effective expiry and an optional
/// per-call hint. `0` / `None` means "contributes no bound".
fn tighten_expiry(effective: u64, hint: Option<u64>) -> u64 {
    match hint {
        Some(hint) if hint != 0 => {
            if effective == 0 {
                hint
            } else {
                effective.min(hint)
            }
        }
        _ => effective,
    }
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
    //   enable_external_commits → create_external_invite → join
    // live in the QR-invite integration test (T-1, libxmtp-integration-test).
    // The L-6 setter routes through the AppDataUpdate intent path; the full
    // commit / decrypt / round-trip surface is best validated in a setting
    // that also exercises the joiner.

    #[test]
    fn tighten_expiry_picks_tightest_active_value() {
        // No hint → effective passes through (including 0 = unbounded).
        assert_eq!(tighten_expiry(5_000, None), 5_000);
        assert_eq!(tighten_expiry(0, None), 0);

        // Hint tighter than effective wins.
        assert_eq!(tighten_expiry(10_000, Some(3_000)), 3_000);

        // Hint looser than effective is ignored.
        assert_eq!(tighten_expiry(3_000, Some(10_000)), 3_000);

        // Hint bounds an otherwise-unbounded blob.
        assert_eq!(tighten_expiry(0, Some(4_000)), 4_000);

        // Zero hint is "unset", never "expire immediately".
        assert_eq!(tighten_expiry(7_000, Some(0)), 7_000);
    }

    #[test]
    fn blob_expiry_folds_staleness_window_from_epoch_start() {
        // The wrap-time fold mirrors the validator's check 9: the
        // staleness deadline is epoch_start + expire_in_ns, NOT
        // upload-time + expire_in_ns.
        let policy = ExternalCommitPolicyV1 {
            expires_at_ns: 100_000,
            expire_in_ns: 500,
            ..Default::default()
        };
        let effective = effective_expires_at_ns(
            policy.expires_at_ns,
            1_000, // epoch began
            policy.expire_in_ns,
        );
        assert_eq!(effective, 1_500);

        // Absolute campaign expiry wins when tighter.
        let effective = effective_expires_at_ns(1_200, 1_000, 500);
        assert_eq!(effective, 1_200);
    }
}
