//! A backend client that changes identity publishes and reads as a test's
//! script says, for identity-load tests here and in the client.

use crate::identity_lifecycle::test_support::PeerApi;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::Notify;
use xmtp_proto::{
    api::ApiClientError, api_client::XmtpBackendClient, backend_v1 as wire, types::TopicKind,
};

/// The error a read stopped by the script fails with.
pub fn stopped() -> ApiClientError {
    ApiClientError::OtherUnretryable(Box::new(tonic::Status::new(
        tonic::Code::Unavailable,
        "stopped by the test",
    )))
}

/// What the test backend does to identity publishes and reads.
#[derive(Default)]
pub struct IdentityScript {
    /// Identity publishes still to refuse as a commit-time conflict.
    pub conflicts: usize,
    /// Answer each identity read with at most one update, and more to follow.
    pub one_per_read: bool,
    /// Identity reads still answered before the next one stops.
    pub reads_before_stop: Option<usize>,
    /// Wake this and hold the stopped read until the caller drops it, instead
    /// of failing the read.
    pub hold: Option<Arc<Notify>>,
    /// Refuse every identity read with a rate limit of this delay.
    pub rate_limit: Option<xmtp_common::time::Duration>,
    /// Answer identity reads from these envelopes instead of the backend.
    pub served: Option<Vec<wire::ServerEnvelope>>,
    /// Identity reads received.
    pub reads: usize,
}

/// Forwards every call to the test backend, changing identity publishes and
/// reads as its script says.
#[derive(Clone)]
pub struct ScriptedIdentity {
    /// The test backend client every call goes to.
    pub inner: PeerApi,
    /// What to change.
    pub script: Arc<Mutex<IdentityScript>>,
}

fn is_identity_read(request: &wire::QueryRequest) -> bool {
    request.queries.iter().any(|query| {
        query.topic.as_ref().and_then(|topic| topic.topic.first())
            == Some(&(TopicKind::IdentityUpdatesV1 as u8))
    })
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for ScriptedIdentity {
    type Error = ApiClientError;

    fn set_limits(&self, limits: Arc<xmtp_configuration::LimitsConfiguration>) {
        self.inner.set_limits(limits)
    }
    async fn publish(
        &self,
        request: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, Self::Error> {
        let identity = request.envelopes.iter().any(|envelope| {
            matches!(
                envelope.payload,
                Some(wire::client_envelope::Payload::IdentityUpdate(_))
            )
        });
        let conflict = identity && {
            let mut script = self.script.lock();
            let conflict = script.conflicts > 0;
            script.conflicts = script.conflicts.saturating_sub(1);
            conflict
        };
        if conflict {
            return Err(ApiClientError::client(
                xmtp_api_grpc::error::GrpcError::Status(tonic::Status::aborted(
                    "identity conflict",
                )),
            ));
        }
        self.inner.publish(request).await
    }
    async fn create_upload(
        &self,
        request: wire::CreateUploadRequest,
    ) -> Result<wire::CreateUploadResponse, Self::Error> {
        self.inner.create_upload(request).await
    }
    async fn query(&self, request: wire::QueryRequest) -> Result<wire::QueryResponse, Self::Error> {
        if !is_identity_read(&request) {
            return self.inner.query(request).await;
        }
        let rate_limit = {
            let mut script = self.script.lock();
            script.reads += 1;
            script.rate_limit
        };
        if let Some(delay) = rate_limit {
            return Err(ApiClientError::client(
                xmtp_api_grpc::error::GrpcError::Status(xmtp_api_grpc::error::rate_limited_status(
                    "request", delay,
                )),
            ));
        }
        if let Some(served) = self.script.lock().served.clone() {
            let envelopes = served
                .into_iter()
                .filter(|envelope| {
                    let meta = envelope.meta.as_ref().expect("an envelope with metadata");
                    request.queries.iter().any(|query| {
                        query.topic == meta.topic
                            && query.cursor.as_ref().expect("a query cursor").sequence_id
                                < meta
                                    .cursor
                                    .as_ref()
                                    .expect("an envelope cursor")
                                    .sequence_id
                    })
                })
                .collect();
            return Ok(wire::QueryResponse {
                envelopes,
                continuation: Some(wire::Continuation { has_more: false }),
            });
        }
        let (stop, hold, one_per_read) = {
            let mut script = self.script.lock();
            let stop = script.reads_before_stop == Some(0);
            script.reads_before_stop = script
                .reads_before_stop
                .and_then(|reads| reads.checked_sub(1));
            (stop, script.hold.clone(), script.one_per_read)
        };
        if stop {
            if let Some(hold) = hold {
                hold.notify_one();
                return std::future::pending().await;
            }
            return Err(stopped());
        }
        let mut response = self.inner.query(request).await?;
        if one_per_read && response.envelopes.len() > 1 {
            response.envelopes.truncate(1);
            response.continuation = Some(wire::Continuation { has_more: true });
        }
        Ok(response)
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
