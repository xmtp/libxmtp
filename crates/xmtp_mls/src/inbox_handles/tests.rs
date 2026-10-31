//! Handle tests. Tests on the shared backend use random handles and claim at
//! most once per inbox, because a claimed handle is permanent there. Tests
//! that claim more than once use an ephemeral backend.

mod claims;
mod events;
#[cfg(not(target_arch = "wasm32"))]
mod members;
mod resolver;
mod routes;
mod search;

use crate::{Client, context::XmtpSharedContext, utils::Tester};
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
use xmtp_id::associations::{AssociationState, Identifier, handle::Handle};
use xmtp_proto::{api::ApiClientError, api_client::XmtpBackendClient, backend_v1 as wire};

/// Longer than the backend's interval of 7 days between two claims by one
/// inbox. Aging an inbox's identity history by it admits the inbox's next claim.
#[cfg(not(target_arch = "wasm32"))]
const PAST_CLAIM_INTERVAL: std::time::Duration = std::time::Duration::from_secs(8 * 24 * 60 * 60);

/// A handle that no other test run claims.
fn random_handle() -> Handle {
    let suffix = hex::encode(xmtp_common::rand_vec::<10>());
    Handle::parse(&format!("t{suffix}")).expect("a random handle matches the grammar")
}

/// Claim `handle` for the tester's inbox, signed by the tester's wallet.
/// `Client::claim_handle` signs with the installation key, and the signature
/// text has one-second precision. So a switch back to a handle in the same
/// second by that key repeats its earlier signature, which replay protection
/// rejects. A second signer avoids that without a wait.
#[cfg(not(target_arch = "wasm32"))]
async fn claim_with_wallet(
    tester: &Tester,
    handle: &Handle,
) -> Result<(), crate::client::ClientError> {
    use crate::identity_updates::IdentityUpdates;
    use xmtp_id::associations::{
        builder::SignatureRequestBuilder,
        test_utils::{WalletTestExt, add_wallet_signature},
    };
    let wallet = &tester.builder.owner;
    let mut request = SignatureRequestBuilder::new(tester.inbox_id())
        .claim_handle(handle.clone(), wallet.member_identifier())
        .build();
    add_wallet_signature(&mut request, wallet).await;
    IdentityUpdates::new(&tester.context)
        .apply_signature_request(request)
        .await
}

/// Wait until the backend's lookup names an inbox for `handle`. A lookup reads
/// the backend's replica, which can lag behind a claim that the primary just
/// stored. Call this after a claim on the shared backend and before anything
/// that looks the handle up.
async fn wait_until_claim_visible<Context: XmtpSharedContext>(
    client: &Client<Context>,
    handle: &Handle,
) {
    xmtp_common::wait_for_eq(
        || async {
            client
                .handles_have_owner(std::slice::from_ref(handle))
                .await
                .ok()
        },
        Some(vec![true]),
    )
    .await
    .expect("the lookup sees the claim within 20 s");
}

/// `base` after claims of `handles`, in order. It is built in memory, for
/// tests that store states directly.
fn state_with_handles(base: &AssociationState, handles: &[&str]) -> AssociationState {
    handles.iter().fold(base.clone(), |state, handle| {
        state.claim_handle(Handle::parse(handle).expect("test handles match the grammar"))
    })
}

/// A new inbox's association state, with no handles.
fn new_inbox_state() -> AssociationState {
    AssociationState::new(Identifier::rand_ethereum(), 0, None).expect("a new inbox state")
}

/// What a [`ScriptedLookups`] client saw, and the inboxes it names falsely.
#[derive(Default)]
struct LookupScript {
    /// Lookups of an identifier, a handle or an account, answered with this
    /// inbox, as a lying backend would.
    lies: HashMap<String, String>,
    /// The number of entries in each `GetInboxIds` request, in order.
    lookups: Vec<usize>,
    /// The number of `Publish` requests.
    publishes: usize,
}

/// A backend client that forwards every call to the shared backend. It
/// records lookups and publishes, and can answer a handle or account lookup
/// with a chosen inbox.
#[derive(Clone)]
struct ScriptedLookups {
    inner: crate::utils::TestClient,
    script: Arc<Mutex<LookupScript>>,
}

type ScriptedContext = Arc<
    crate::context::XmtpMlsLocalContext<
        ScriptedLookups,
        xmtp_db::DefaultStore,
        crate::utils::TestMlsStorage,
    >,
>;

/// A second client of `base`'s inbox and database, whose requests go through
/// a [`ScriptedLookups`].
async fn scripted_client(
    base: &Tester,
) -> Result<
    (crate::Client<ScriptedContext>, Arc<Mutex<LookupScript>>),
    crate::builder::ClientBuilderError,
> {
    let script = Arc::new(Mutex::new(LookupScript::default()));
    let api = ScriptedLookups {
        inner: base.context.api().api_client.raw_for_test().clone(),
        script: script.clone(),
    };
    let client = crate::builder::ClientBuilder::from_client(base.client.clone())
        .api_client(api)
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    Ok((client, script))
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for ScriptedLookups {
    type Error = ApiClientError;

    async fn publish(
        &self,
        request: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, Self::Error> {
        self.script.lock().publishes += 1;
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
        self.script.lock().lookups.push(request.requests.len());
        let mut response = self.inner.get_inbox_ids(request).await?;
        let script = self.script.lock();
        for answer in &mut response.responses {
            if let Some(inbox_id) = script.lies.get(&answer.identifier) {
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
    fn backend_url(&self) -> Option<&str> {
        self.inner.backend_url()
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
