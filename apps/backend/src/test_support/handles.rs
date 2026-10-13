//! Inboxes that claim handles, for the backend's handle tests.

use super::{RunningServer, TestResult};
use crate::api;
use xmtp_cryptography::{XmtpInstallationCredential, utils::generate_local_wallet};
use xmtp_id::associations::{
    Handle, MemberIdentifier,
    builder::SignatureRequestBuilder,
    test_utils::{WalletTestExt, add_installation_key_signature, add_wallet_signature},
};
use xmtp_mls_validation::test_utils::identity_envelope;
use xmtp_proto::xmtp::identity::associations::{IdentifierKind, IdentityUpdate};

/// A wallet's inbox with one installation, which signs the inbox's claims.
pub(crate) struct ClaimingInbox {
    installation: XmtpInstallationCredential,
    /// The inbox id as lowercase hex.
    pub inbox_id: String,
}

impl ClaimingInbox {
    /// A new inbox, created on `server` with its installation registered.
    pub(crate) async fn registered(server: &RunningServer) -> TestResult<Self> {
        let wallet = generate_local_wallet();
        let inbox = Self {
            inbox_id: wallet.get_inbox_id(0),
            installation: XmtpInstallationCredential::new(),
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

    /// An update in which the installation claims `handle`. Each call takes
    /// a new timestamp, so two calls give two updates with two replay keys.
    pub(crate) async fn claim(&self, handle: &str) -> IdentityUpdate {
        let handle = Handle::parse(handle).expect("the test names a valid handle");
        let mut request = SignatureRequestBuilder::new(&self.inbox_id)
            .claim_handle(handle, self.installation_member())
            .build();
        add_installation_key_signature(&mut request, &self.installation).await;
        request
            .build_identity_update()
            .expect("the request is signed")
            .into()
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
                envelopes: vec![identity_envelope(self.claim(handle).await)],
            })
            .await
            .map(tonic::Response::into_inner)
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
