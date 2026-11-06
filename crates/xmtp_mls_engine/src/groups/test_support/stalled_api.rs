//! A backend client that never answers one kind of call, for tests of the
//! deadlines that bound a stalled request.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use xmtp_mls_identity::identity_lifecycle::test_support::PeerApi;
use xmtp_proto::backend_v1::client_envelope::Payload;
use xmtp_proto::{api::ApiClientError, api_client::XmtpBackendClient, backend_v1 as wire};

/// The call a [`StalledApi`] never answers.
#[derive(Clone, Copy)]
pub enum StalledCall {
    /// The target query a round makes when a publish returns no receipts.
    TargetQuery,
    /// A publish that carries a Welcome.
    WelcomePublish,
    /// Nothing stalls; publish counts the group-message and welcome envelopes.
    Recorded,
}

/// A client of the local backend that never answers its stalled call, and
/// records what reached it.
#[derive(Clone)]
pub struct StalledApi {
    /// The backend client that every other call goes to.
    pub inner: PeerApi,
    /// The call that never answers.
    pub call: StalledCall,
    /// Set when the stalled call starts.
    pub entered: Arc<AtomicBool>,
    /// Set when the stalled call's future is dropped.
    pub cancelled: Arc<AtomicBool>,
    /// The group-message envelopes published while recording.
    pub group_messages: Arc<AtomicUsize>,
    /// The Welcome envelopes published while recording.
    pub welcomes: Arc<AtomicUsize>,
}

impl StalledApi {
    async fn never_reply<T>(&self) -> T {
        struct RequestGuard(Arc<AtomicBool>);
        impl Drop for RequestGuard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _request = RequestGuard(self.cancelled.clone());
        self.entered.store(true, Ordering::SeqCst);
        futures::future::pending().await
    }
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for StalledApi {
    type Error = ApiClientError;

    async fn publish(
        &self,
        request: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, Self::Error> {
        match self.call {
            // Missing receipts leave the prepared attempt unresolved, so the
            // round must query its target before it can receive messages.
            StalledCall::TargetQuery => Ok(wire::PublishResponse::default()),
            StalledCall::WelcomePublish
                if request.envelopes.iter().any(|envelope| {
                    matches!(envelope.payload, Some(Payload::WelcomeMessage(_)))
                }) =>
            {
                self.never_reply().await
            }
            StalledCall::WelcomePublish => self.inner.publish(request).await,
            StalledCall::Recorded => {
                for envelope in &request.envelopes {
                    match envelope.payload {
                        Some(Payload::GroupMessage(_)) => {
                            self.group_messages.fetch_add(1, Ordering::SeqCst);
                        }
                        Some(Payload::WelcomeMessage(_)) => {
                            self.welcomes.fetch_add(1, Ordering::SeqCst);
                        }
                        _ => {}
                    }
                }
                self.inner.publish(request).await
            }
        }
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
        match self.call {
            StalledCall::TargetQuery => self.never_reply().await,
            StalledCall::WelcomePublish | StalledCall::Recorded => {
                self.inner.query_newest(request).await
            }
        }
    }

    async fn get_inbox_ids(
        &self,
        request: wire::GetInboxIdsRequest,
    ) -> Result<wire::GetInboxIdsResponse, Self::Error> {
        self.inner.get_inbox_ids(request).await
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
