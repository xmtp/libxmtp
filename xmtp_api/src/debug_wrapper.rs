use std::future::Future;
use xmtp_common::RetryableError;
use xmtp_proto::api_client::AggregateStats;
use xmtp_proto::api_client::ApiStats;
use xmtp_proto::api_client::IdentityStats;
use xmtp_proto::mls_v1::{
    BatchPublishCommitLogRequest, BatchQueryCommitLogRequest, BatchQueryCommitLogResponse,
};
use xmtp_proto::xmtp::identity::api::v1::GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request;
use xmtp_proto::xmtp::identity::api::v1::GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response;
use xmtp_proto::xmtp::identity::api::v1::GetInboxIdsRequest;
use xmtp_proto::xmtp::identity::api::v1::GetInboxIdsResponse;
use xmtp_proto::xmtp::identity::api::v1::PublishIdentityUpdateRequest;
use xmtp_proto::xmtp::identity::api::v1::PublishIdentityUpdateResponse;
use xmtp_proto::xmtp::identity::api::v1::VerifySmartContractWalletSignaturesRequest;
use xmtp_proto::xmtp::identity::api::v1::VerifySmartContractWalletSignaturesResponse;
use xmtp_proto::xmtp::mls::api::v1::FetchKeyPackagesRequest;
use xmtp_proto::xmtp::mls::api::v1::FetchKeyPackagesResponse;
use xmtp_proto::xmtp::mls::api::v1::QueryGroupMessagesRequest;
use xmtp_proto::xmtp::mls::api::v1::QueryGroupMessagesResponse;
use xmtp_proto::xmtp::mls::api::v1::QueryWelcomeMessagesRequest;
use xmtp_proto::xmtp::mls::api::v1::QueryWelcomeMessagesResponse;
use xmtp_proto::xmtp::mls::api::v1::SendGroupMessagesRequest;
use xmtp_proto::xmtp::mls::api::v1::SendWelcomeMessagesRequest;
use xmtp_proto::xmtp::mls::api::v1::SubscribeGroupMessagesRequest;
use xmtp_proto::xmtp::mls::api::v1::SubscribeWelcomeMessagesRequest;
use xmtp_proto::xmtp::mls::api::v1::UploadKeyPackageRequest;
use xmtp_proto::{
    api::{ApiClientError, HasStats},
    prelude::{XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams},
};

#[derive(Clone)]
pub struct ApiDebugWrapper<A> {
    inner: A,
}

impl<A> ApiDebugWrapper<A> {
    pub fn new(api: A) -> Self {
        Self { inner: api }
    }
}

async fn wrap_err<T, R, F, E>(
    req: R,
    stats: impl Fn() -> AggregateStats,
) -> Result<T, ApiClientError<E>>
where
    R: FnOnce() -> F,
    F: Future<Output = Result<T, ApiClientError<E>>>,
    E: std::error::Error + RetryableError + Send + Sync + 'static,
{
    let res = req().await;
    if let Err(e) = res {
        match e {
            ApiClientError::ClientWithEndpoint { endpoint, source } => {
                Err(ApiClientError::ClientWithEndpointAndStats {
                    endpoint,
                    source,
                    stats: stats(),
                })
            }
            err @ ApiClientError::ClientWithEndpointAndStats { .. } => Err(err),
            e => Err(ApiClientError::ErrorWithStats {
                e: Box::new(e),
                stats: stats(),
            }),
        }
    } else {
        res
    }
}

impl<A> HasStats for ApiDebugWrapper<A>
where
    A: HasStats,
{
    fn aggregate_stats(&self) -> AggregateStats {
        self.inner.aggregate_stats()
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<A, E> XmtpMlsClient for ApiDebugWrapper<A>
where
    A: XmtpMlsClient<Error = ApiClientError<E>> + Send + Sync,
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    A: HasStats,
{
    type Error = ApiClientError<E>;

    async fn upload_key_package(
        &self,
        request: UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        wrap_err(
            || self.inner.upload_key_package(request),
            || self.inner.aggregate_stats(),
        )
        .await
    }

    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Self::Error> {
        wrap_err(
            || self.inner.fetch_key_packages(request),
            || self.inner.aggregate_stats(),
        )
        .await
    }

    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        wrap_err(
            || self.inner.send_group_messages(request),
            || self.inner.aggregate_stats(),
        )
        .await
    }

    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        wrap_err(
            || self.inner.send_welcome_messages(request),
            || self.inner.aggregate_stats(),
        )
        .await
    }

    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Self::Error> {
        wrap_err(
            || self.inner.query_group_messages(request),
            || self.inner.aggregate_stats(),
        )
        .await
    }

    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Self::Error> {
        wrap_err(
            || self.inner.query_welcome_messages(request),
            || self.inner.aggregate_stats(),
        )
        .await
    }

    async fn publish_commit_log(
        &self,
        request: BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        wrap_err(
            || self.inner.publish_commit_log(request),
            || self.inner.aggregate_stats(),
        )
        .await
    }

    async fn query_commit_log(
        &self,
        request: BatchQueryCommitLogRequest,
    ) -> Result<BatchQueryCommitLogResponse, Self::Error> {
        wrap_err(
            || self.inner.query_commit_log(request),
            || self.inner.aggregate_stats(),
        )
        .await
    }

    fn stats(&self) -> ApiStats {
        self.inner.stats()
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<A, E> XmtpMlsStreams for ApiDebugWrapper<A>
where
    A: XmtpMlsStreams<Error = ApiClientError<E>> + Send + Sync + 'static,
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    A: HasStats,
{
    type GroupMessageStream = <A as XmtpMlsStreams>::GroupMessageStream;

    type WelcomeMessageStream = <A as XmtpMlsStreams>::WelcomeMessageStream;

    type Error = ApiClientError<E>;

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
        buffer_size: usize,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        wrap_err(
            || self.inner.subscribe_group_messages(request, buffer_size),
            || self.inner.aggregate_stats(),
        )
        .await
    }

    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        wrap_err(
            || self.inner.subscribe_welcome_messages(request),
            || self.inner.aggregate_stats(),
        )
        .await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<A, E> XmtpIdentityClient for ApiDebugWrapper<A>
where
    A: XmtpIdentityClient<Error = ApiClientError<E>> + Send + Sync,
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    A: HasStats,
{
    type Error = ApiClientError<E>;

    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Self::Error> {
        wrap_err(
            || self.inner.publish_identity_update(request),
            || self.inner.aggregate_stats(),
        )
        .await
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Self::Error> {
        wrap_err(
            || self.inner.get_identity_updates_v2(request),
            || self.inner.aggregate_stats(),
        )
        .await
    }

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        wrap_err(
            || self.inner.get_inbox_ids(request),
            || self.inner.aggregate_stats(),
        )
        .await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        wrap_err(
            || self.inner.verify_smart_contract_wallet_signatures(request),
            || self.inner.aggregate_stats(),
        )
        .await
    }

    fn identity_stats(&self) -> IdentityStats {
        self.inner.identity_stats()
    }
}
