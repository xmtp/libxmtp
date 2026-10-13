//! Inboxes that claim handles, for the backend's handle tests.

use super::{RunningServer, TestResult};
use crate::api;
use std::sync::atomic::{AtomicU64, Ordering};
use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential, utils::generate_local_wallet};
use xmtp_id::associations::{
    Handle, InstallationKeyContext, MemberIdentifier,
    builder::SignatureRequestBuilder,
    test_utils::{WalletTestExt, add_installation_key_signature, add_wallet_signature},
    unsigned_actions::{
        SignatureTextCreator, UnsignedAction, UnsignedClaimHandle, UnsignedIdentityUpdate,
    },
    unverified::{
        UnverifiedAction, UnverifiedClaimHandle, UnverifiedIdentityUpdate, UnverifiedSignature,
    },
};
use xmtp_mls_validation::test_utils::identity_envelope;
use xmtp_proto::xmtp::identity::associations::{IdentifierKind, IdentityUpdate};

/// A wallet's inbox with one installation, which signs the inbox's claims.
pub(crate) struct ClaimingInbox {
    installation: XmtpInstallationCredential,
    /// The inbox id as lowercase hex.
    pub inbox_id: String,
    /// Claims signed so far, which spaces their timestamps.
    signed: AtomicU64,
}

impl ClaimingInbox {
    /// A new inbox, created on `server` with its installation registered.
    pub(crate) async fn registered(server: &RunningServer) -> TestResult<Self> {
        let wallet = generate_local_wallet();
        let inbox = Self {
            inbox_id: wallet.get_inbox_id(0),
            installation: XmtpInstallationCredential::new(),
            signed: AtomicU64::new(0),
        };
        let mut request = SignatureRequestBuilder::new(&inbox.inbox_id)
            .create_inbox(wallet.identifier(), 0)
            .add_association(inbox.installation_member(), wallet.identifier().into())
            .build();
        add_wallet_signature(&mut request, &wallet).await;
        add_installation_key_signature(&mut request, &inbox.installation).await;
        server
            .publish(vec![identity_envelope(
                request.build_identity_update()?.into(),
            )])
            .await?;
        Ok(inbox)
    }

    /// An update in which the installation claims `handle`.
    pub(crate) fn claim(&self, handle: &str) -> IdentityUpdate {
        self.claim_signed_by(handle, &self.installation)
    }

    /// Publish a claim of `handle` and return the status a refusal carries.
    pub(crate) async fn publish_claim(
        &self,
        server: &RunningServer,
        handle: &str,
    ) -> Result<api::PublishResponse, tonic::Status> {
        server
            .publisher()
            .publish(api::PublishRequest {
                envelopes: vec![identity_envelope(self.claim(handle))],
            })
            .await
            .map(tonic::Response::into_inner)
    }

    /// An update for this inbox that claims `handle`, signed by an
    /// installation that is not a member of the inbox.
    pub(crate) fn claim_by_stranger(&self, handle: &str) -> IdentityUpdate {
        self.claim_signed_by(handle, &XmtpInstallationCredential::new())
    }

    /// A claim of `handle` signed by `key`. The signature text shows the time
    /// to the second, and an installation key signs one text the same way
    /// twice, so each claim takes a timestamp one second past the previous
    /// one. A test can then claim one handle twice without a replay.
    fn claim_signed_by(&self, handle: &str, key: &XmtpInstallationCredential) -> IdentityUpdate {
        let handle = Handle::parse(handle).expect("the test names a valid handle");
        let offset = self.signed.fetch_add(1, Ordering::Relaxed) * xmtp_common::NS_IN_SEC as u64;
        let timestamp = xmtp_common::time::now_ns() as u64 + offset;
        let unsigned = UnsignedClaimHandle {
            handle: handle.into(),
        };
        let text = UnsignedIdentityUpdate::new(
            vec![UnsignedAction::ClaimHandle(unsigned.clone())],
            self.inbox_id.clone(),
            timestamp,
        )
        .signature_text();
        let signature = key
            .credential_sign::<InstallationKeyContext>(text)
            .expect("the installation key signs");
        UnverifiedIdentityUpdate::new(
            self.inbox_id.clone(),
            timestamp,
            vec![UnverifiedAction::ClaimHandle(UnverifiedClaimHandle::new(
                unsigned,
                UnverifiedSignature::new_installation_key(signature, key.verifying_key()),
            ))],
        )
        .into()
    }

    /// An update in which the installation adds a new wallet to the inbox.
    pub(crate) async fn add_wallet(&self) -> IdentityUpdate {
        let wallet = generate_local_wallet();
        let mut request = SignatureRequestBuilder::new(&self.inbox_id)
            .add_association(wallet.member_identifier(), self.installation_member())
            .build();
        add_installation_key_signature(&mut request, &self.installation).await;
        add_wallet_signature(&mut request, &wallet).await;
        request
            .build_identity_update()
            .expect("the request is signed")
            .into()
    }

    fn installation_member(&self) -> MemberIdentifier {
        MemberIdentifier::installation(self.installation.public_bytes().to_vec())
    }
}

/// The inbox each handle resolves to, in order, as lowercase hex.
pub(crate) async fn resolve(
    server: &RunningServer,
    handles: &[&str],
) -> Result<Vec<Option<String>>, tonic::Status> {
    let requests = handles
        .iter()
        .map(|handle| api::get_inbox_ids_request::Request {
            identifier: handle.to_string(),
            identifier_kind: IdentifierKind::Handle.into(),
        })
        .collect();
    Ok(server
        .identity()
        .get_inbox_ids(api::GetInboxIdsRequest { requests })
        .await?
        .into_inner()
        .responses
        .into_iter()
        .map(|response| response.inbox_id)
        .collect())
}
