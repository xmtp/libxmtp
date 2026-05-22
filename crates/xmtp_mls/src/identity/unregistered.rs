//! Two-phase identity construction split from [`Identity::new`].
//!
//! Today's `Identity::new` mixes pure crypto, network reads, and (on the
//! legacy path) network writes. This module separates them:
//!
//! - [`UnregisteredIdentity::generate`] — pure crypto (keypair + credential).
//! - [`UnregisteredIdentity::attest`] — network read that classifies the
//!   inbox state and pre-signs with the installation key.
//! - [`UnregisteredIdentity::publish`] — network write that uploads the
//!   key package, publishes the identity update, persists `StoredIdentity`,
//!   and returns a registered [`Identity`].
//!
//! `Identity::new` is preserved as a backward-compat shim that drives this
//! new API internally. Callers can migrate incrementally.
use super::{Identity, IdentityError, create_credential};
use crate::XmtpApi;
use crate::identity_updates::{get_association_state_with_verifier, load_identity_updates};
use std::sync::atomic::AtomicBool;
use xmtp_api::ApiClientWrapper;
use xmtp_configuration::MAX_INSTALLATIONS_PER_INBOX;
use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential};
use xmtp_db::prelude::*;
use xmtp_id::InboxId;
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_id::associations::{
    Identifier, InstallationKeyContext, MemberIdentifier,
    builder::{SignatureRequest, SignatureRequestBuilder},
    sign_with_legacy_key,
};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use zeroize::Zeroizing;

/// Local-only identity material. No network calls performed yet.
///
/// Drive through the lifecycle by calling [`attest`](Self::attest) (network
/// read, classifies the inbox) then either [`publish`](Self::publish)
/// (network write, completes registration) — or, for branches that need an
/// external wallet signature, add signatures via
/// [`pending_signature`](Self::pending_signature) before calling `publish`.
pub struct UnregisteredIdentity {
    inbox_id: InboxId,
    identifier: Identifier,
    nonce: u64,
    /// V2 legacy signed private key (when migrating an existing V2
    /// inbox). Wrapped in `Zeroizing` so the bytes are wiped from
    /// memory when the `UnregisteredIdentity` is dropped.
    legacy_signed_private_key: Option<Zeroizing<Vec<u8>>>,
    installation_keys: XmtpInstallationCredential,
    pending: AttestationState,
}

/// Lifecycle state for an [`UnregisteredIdentity`].
enum AttestationState {
    /// Just generated. Must call `attest` before `publish`.
    NotAttested,
    /// Attested. Variants describe how to complete publishing.
    Attested(PendingAttestation),
}

/// Network-discovered status of the inbox at attest time. Each variant
/// carries the in-progress [`SignatureRequest`] that completes that
/// branch's identity update.
pub enum PendingAttestation {
    /// Inbox already exists on network. Caller must add a wallet
    /// signature to the [`SignatureRequest`] before calling
    /// [`UnregisteredIdentity::publish`].
    JoiningExistingInbox(SignatureRequest),
    /// Fresh inbox claim. Caller must add a wallet signature before
    /// publishing.
    ClaimingNewInbox(SignatureRequest),
    /// Fresh inbox claim signed by a legacy V2 key — no external
    /// signature needed. Ready to publish immediately.
    ReadyToPublish(SignatureRequest),
}

impl UnregisteredIdentity {
    /// Pure crypto: generates an installation keypair and an OpenMLS
    /// credential. Does not touch the network.
    ///
    /// `inbox_id` is the caller-provided identifier the on-network state
    /// will be validated against during [`attest`](Self::attest).
    pub fn generate(
        inbox_id: InboxId,
        identifier: Identifier,
        nonce: u64,
        legacy_signed_private_key: Option<Vec<u8>>,
    ) -> Result<Self, IdentityError> {
        Ok(Self {
            inbox_id,
            identifier,
            nonce,
            legacy_signed_private_key: legacy_signed_private_key.map(Zeroizing::new),
            installation_keys: XmtpInstallationCredential::new(),
            pending: AttestationState::NotAttested,
        })
    }

    /// Network read: classify the inbox state, construct the appropriate
    /// [`PendingAttestation`], and pre-sign the request with the
    /// installation key.
    ///
    /// Idempotent: calling twice re-runs the network classification and
    /// rebuilds the signature request.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn attest<C: XmtpApi, S: XmtpMlsStorageProvider>(
        &mut self,
        api_client: &ApiClientWrapper<C>,
        mls_storage: &S,
        scw_signature_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<(), IdentityError> {
        let pending = match self.lookup_associated_inbox(api_client).await? {
            Some(inbox) => {
                self.guard_inbox_match(&inbox)?;
                self.load_and_check_installations(api_client, mls_storage, &scw_signature_verifier)
                    .await?;
                let req = self
                    .build_and_presign(SignatureRequestBuilder::new(inbox), &scw_signature_verifier)
                    .await?;
                PendingAttestation::JoiningExistingInbox(req)
            }
            None if self.legacy_signed_private_key.is_some() => {
                self.guard_legacy_preconditions()?;
                let legacy = self.legacy_signed_private_key.clone().expect("checked");
                let mut req = self
                    .build_and_presign(self.fresh_builder(), &scw_signature_verifier)
                    .await?;
                self.add_legacy_signature(&mut req, legacy, &scw_signature_verifier)
                    .await?;
                PendingAttestation::ReadyToPublish(req)
            }
            None => {
                self.ensure_generated_inbox_id_matches()?;
                let req = self
                    .build_and_presign(self.fresh_builder(), &scw_signature_verifier)
                    .await?;
                PendingAttestation::ClaimingNewInbox(req)
            }
        };

        self.pending = AttestationState::Attested(pending);
        Ok(())
    }

    async fn lookup_associated_inbox<C: XmtpApi>(
        &self,
        api_client: &ApiClientWrapper<C>,
    ) -> Result<Option<InboxId>, IdentityError> {
        let inbox_ids = api_client
            .get_inbox_ids(vec![self.identifier.clone().into()])
            .await?;
        Ok(inbox_ids.get(&(&self.identifier).into()).cloned())
    }

    fn guard_inbox_match(&self, associated: &InboxId) -> Result<(), IdentityError> {
        if *associated != self.inbox_id {
            return Err(IdentityError::NewIdentity("Inbox ID mismatch".to_string()));
        }
        Ok(())
    }

    fn guard_legacy_preconditions(&self) -> Result<(), IdentityError> {
        if self.nonce != 0 {
            return Err(IdentityError::NewIdentity(
                "Nonce must be 0 if legacy key is provided".to_string(),
            ));
        }
        self.ensure_generated_inbox_id_matches()
    }

    async fn load_and_check_installations<C: XmtpApi, S: XmtpMlsStorageProvider>(
        &self,
        api_client: &ApiClientWrapper<C>,
        mls_storage: &S,
        scw_signature_verifier: &impl SmartContractSignatureVerifier,
    ) -> Result<(), IdentityError> {
        load_identity_updates(api_client, &mls_storage.db(), &[self.inbox_id.as_str()])
            .await
            .map_err(|e| {
                IdentityError::NewIdentity(format!("Failed to load identity updates: {e}"))
            })?;
        let state = get_association_state_with_verifier(
            &mls_storage.db(),
            &self.inbox_id,
            None,
            scw_signature_verifier,
        )
        .await
        .map_err(|err| {
            IdentityError::NewIdentity(format!("Error resolving identity state: {err}"))
        })?;
        let count = state.installation_ids().len();
        if count >= MAX_INSTALLATIONS_PER_INBOX {
            return Err(IdentityError::TooManyInstallations {
                inbox_id: self.inbox_id.clone(),
                count,
                max: MAX_INSTALLATIONS_PER_INBOX,
            });
        }
        Ok(())
    }

    fn fresh_builder(&self) -> SignatureRequestBuilder {
        SignatureRequestBuilder::new(self.inbox_id.clone())
            .create_inbox(self.identifier.clone(), self.nonce)
    }

    async fn build_and_presign(
        &self,
        builder: SignatureRequestBuilder,
        scw_signature_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<SignatureRequest, IdentityError> {
        let mut req = builder
            .add_association(
                MemberIdentifier::installation(self.installation_keys.public_slice().to_vec()),
                self.identifier.clone().into(),
            )
            .build();
        self.presign_with_installation_key(&mut req, scw_signature_verifier)
            .await?;
        Ok(req)
    }

    async fn add_legacy_signature(
        &self,
        req: &mut SignatureRequest,
        legacy: Zeroizing<Vec<u8>>,
        scw_signature_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<(), IdentityError> {
        req.add_signature(
            UnverifiedSignature::LegacyDelegated(sign_with_legacy_key(
                req.signature_text(),
                &legacy,
            )?),
            scw_signature_verifier,
        )
        .await?;
        Ok(())
    }

    fn ensure_generated_inbox_id_matches(&self) -> Result<(), IdentityError> {
        let generated = self.identifier.inbox_id(self.nonce)?;
        if self.inbox_id != generated {
            return Err(IdentityError::NewIdentity(
                "Inbox ID doesn't match nonce & address".to_string(),
            ));
        }
        Ok(())
    }

    async fn presign_with_installation_key(
        &self,
        req: &mut SignatureRequest,
        scw_signature_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<(), IdentityError> {
        let sig = self
            .installation_keys
            .credential_sign::<InstallationKeyContext>(req.signature_text())?;
        req.add_signature(
            UnverifiedSignature::new_installation_key(sig, self.installation_keys.verifying_key()),
            scw_signature_verifier,
        )
        .await?;
        Ok(())
    }

    /// In-progress signature request the caller (wallet / SCW) must add
    /// signatures to. Returns `None` if this branch is already complete
    /// (legacy V2 path) or if [`attest`](Self::attest) hasn't run yet.
    pub fn pending_signature(&mut self) -> Option<&mut SignatureRequest> {
        match &mut self.pending {
            AttestationState::Attested(
                PendingAttestation::JoiningExistingInbox(req)
                | PendingAttestation::ClaimingNewInbox(req),
            ) => Some(req),
            _ => None,
        }
    }

    /// True iff the legacy V2 path applies and `publish` requires no
    /// external signatures.
    pub fn can_self_publish(&self) -> bool {
        matches!(
            self.pending,
            AttestationState::Attested(PendingAttestation::ReadyToPublish(_))
        )
    }

    /// Network write: uploads the key package, publishes the identity
    /// update, persists `StoredIdentity`. Consumes self.
    ///
    /// Requires [`attest`](Self::attest) to have been called. For
    /// `JoiningExistingInbox` / `ClaimingNewInbox`, the signature
    /// request returned by [`pending_signature`](Self::pending_signature)
    /// must be completed (wallet signature added) before calling this.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn publish<C: XmtpApi, S: XmtpMlsStorageProvider>(
        self,
        api_client: &ApiClientWrapper<C>,
        mls_storage: &S,
    ) -> Result<Identity, IdentityError> {
        let Self {
            inbox_id,
            installation_keys,
            pending,
            ..
        } = self;
        let AttestationState::Attested(pending) = pending else {
            return Err(IdentityError::NewIdentity(
                "publish() called before attest()".to_string(),
            ));
        };
        let signature_request = match pending {
            PendingAttestation::JoiningExistingInbox(req)
            | PendingAttestation::ClaimingNewInbox(req)
            | PendingAttestation::ReadyToPublish(req) => req,
        };

        // Build the identity update first so any signature-missing
        // error (e.g. caller forgot to add a wallet signature) surfaces
        // BEFORE `register()` performs its irreversible key-package
        // upload + StoredIdentity persist. Without this ordering, a
        // failed `build_identity_update()` would leave the system with
        // a registered identity but no published update.
        let credential = create_credential(signature_request.inbox_id())?;
        let identity_update = signature_request.build_identity_update()?;

        let identity = Identity {
            inbox_id,
            installation_keys,
            credential,
            signature_request: None,
            is_ready: AtomicBool::new(true),
        };

        identity
            .persist_key_package(api_client, mls_storage)
            .await?;
        api_client.publish_identity_update(identity_update).await?;

        Ok(identity)
    }

    /// Compat: flatten state into the legacy [`Identity`] shape with
    /// `signature_request: Some` and `is_ready: false`. Used by
    /// `Identity::new` to preserve its pre-refactor return contract for
    /// branches that require an external wallet signature.
    pub(crate) fn into_legacy_shim(self) -> Result<Identity, IdentityError> {
        let Self {
            inbox_id,
            installation_keys,
            pending,
            ..
        } = self;
        let AttestationState::Attested(pending) = pending else {
            return Err(IdentityError::NewIdentity(
                "into_legacy_shim() called before attest()".to_string(),
            ));
        };
        let signature_request = match pending {
            PendingAttestation::JoiningExistingInbox(req)
            | PendingAttestation::ClaimingNewInbox(req) => req,
            PendingAttestation::ReadyToPublish(_) => {
                return Err(IdentityError::NewIdentity(
                    "ReadyToPublish must go through publish(), not legacy shim".to_string(),
                ));
            }
        };
        Ok(Identity {
            credential: create_credential(signature_request.inbox_id())?,
            inbox_id,
            installation_keys,
            signature_request: Some(signature_request),
            is_ready: AtomicBool::new(false),
        })
    }
}
