//! The static lookups, which have no client, resolve a handle through the
//! verified resolver for a caller with no client.

use super::*;
use xmtp_id::associations::handle::Handle;
use xmtp_mls::inbox_handles::IdentityTarget;
use xmtp_proto::{
    api_client::XmtpBackendClient, backend_v1 as wire, xmtp::identity::associations::IdentifierKind,
};

type Inner = xmtp_mls::XmtpApiClient;

/// A backend client that forwards every call to a real backend, but answers
/// each listed handle with a chosen inbox, as a lying backend would.
struct LyingLookups {
    inner: Inner,
    lies: HashMap<String, String>,
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for LyingLookups {
    type Error = <Inner as XmtpBackendClient>::Error;

    async fn publish(
        &self,
        request: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, Self::Error> {
        self.inner.publish(request).await
    }
    async fn create_upload(
        &self,
        request: wire::CreateUploadRequest,
    ) -> Result<wire::CreateUploadResponse, Self::Error> {
        self.inner.create_upload(request).await
    }
    async fn query(&self, request: wire::QueryRequest) -> Result<wire::QueryResponse, Self::Error> {
        self.inner.query(request).await
    }
    async fn query_raw_envelopes(
        &self,
        request: wire::QueryRequest,
    ) -> Result<xmtp_proto::types::ReceivedQueryResponse, Self::Error> {
        self.query(request).await.map(Into::into)
    }
    async fn query_newest(
        &self,
        request: wire::QueryNewestRequest,
    ) -> Result<wire::QueryNewestResponse, Self::Error> {
        self.inner.query_newest(request).await
    }
    async fn get_inbox_ids(
        &self,
        request: wire::GetInboxIdsRequest,
    ) -> Result<wire::GetInboxIdsResponse, Self::Error> {
        let mut response = self.inner.get_inbox_ids(request).await?;
        for answer in &mut response.responses {
            if answer.identifier_kind == IdentifierKind::Handle as i32
                && let Some(inbox_id) = self.lies.get(&answer.identifier)
            {
                answer.inbox_id = Some(inbox_id.clone());
            }
        }
        Ok(response)
    }
    async fn get_configuration(
        &self,
        request: wire::GetConfigurationRequest,
    ) -> Result<wire::GetConfigurationResponse, Self::Error> {
        self.inner.get_configuration(request).await
    }
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: wire::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<wire::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.inner
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
    async fn register(
        &self,
        request: wire::RegisterRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.inner.register(request).await
    }
    async fn unregister(
        &self,
        request: wire::UnregisterRequest,
    ) -> Result<wire::UnregisterResponse, Self::Error> {
        self.inner.unregister(request).await
    }
    async fn update_subscriptions(
        &self,
        request: wire::UpdateSubscriptionsRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.inner.update_subscriptions(request).await
    }
}

/// A static API on the shared backend that answers `lies`.
async fn lying_api(
    lies: HashMap<String, String>,
) -> Result<xmtp_api::ApiClientWrapper<LyingLookups>, XmtpError> {
    let Some(BackendSource::Options { options }) = options().backend else {
        panic!("test options name backend options");
    };
    let backend = crate::Backend::connect(options).await?;
    Ok(xmtp_api::ApiClientWrapper::new(
        LyingLookups {
            inner: backend.api.clone(),
            lies,
        },
        Default::default(),
    ))
}

// verifies: IDENT-085
#[xmtp_common::test(unwrap_try = true)]
async fn static_handle_lookup_drops_an_answer_the_inbox_did_not_claim() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let claimed = random_handle();
    claim_visible(&alix, &claimed).await?;
    let unclaimed = random_handle();
    // The backend names Bo for a handle Bo never claimed, and for Alix's
    // handle.
    let api = lying_api(HashMap::from([
        (unclaimed.clone(), bo.inbox_id().into_checked()?),
        (claimed.clone(), bo.inbox_id().into_checked()?),
    ]))
    .await?;
    for handle in [&unclaimed, &claimed] {
        let target = IdentityTarget::Handle(Handle::parse(handle)?);
        let error = crate::static_helpers::inbox_id_for_target(&api, target)
            .await
            .expect_err("a lie names no inbox");
        assert!(
            matches!(error, XmtpError::IdentityNotFound(_)),
            "got {error:?}"
        );
    }

    // An honest answer still resolves.
    let honest = lying_api(HashMap::new()).await?;
    let target = IdentityTarget::Handle(Handle::parse(&claimed)?);
    assert_eq!(
        crate::static_helpers::inbox_id_for_target(&honest, target).await?,
        alix.inbox_id()
    );
    alix.end().await?;
    bo.end().await?;
}

// The availability report checks no state: it reports what the backend
// answers, keyed by the canonical handle.
#[xmtp_common::test(unwrap_try = true)]
async fn static_handle_availability_reports_the_backend_answer() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let claimed = random_handle();
    claim_visible(&alix, &claimed).await?;
    let unclaimed = random_handle();
    let result = crate::static_helpers::can_message_with_backend(
        shared_source(),
        vec![
            handle_identity(&claimed),
            handle_identity(&unclaimed.to_uppercase()),
        ],
    )
    .await?;
    assert_eq!(
        result,
        HashMap::from([
            (format!("handle:{claimed}"), true),
            (format!("handle:{unclaimed}"), false),
        ])
    );
    alix.end().await?;
}
