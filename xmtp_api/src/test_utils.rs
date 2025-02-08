#![allow(clippy::unwrap_used)]

use mockall::mock;
use xmtp_proto::{
    api_client::{ClientWithMetadata, XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams},
    xmtp::{
        identity::api::v1::{
            GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
            GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
            GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
            VerifySmartContractWalletSignaturesRequest,
            VerifySmartContractWalletSignaturesResponse,
        },
        mls::api::v1::{
            group_message::{Version as GroupMessageVersion, V1 as GroupMessageV1},
            FetchKeyPackagesRequest, FetchKeyPackagesResponse, GroupMessage,
            QueryGroupMessagesRequest, QueryGroupMessagesResponse, QueryWelcomeMessagesRequest,
            QueryWelcomeMessagesResponse, SendGroupMessagesRequest, SendWelcomeMessagesRequest,
            SubscribeGroupMessagesRequest, SubscribeWelcomeMessagesRequest,
            UploadKeyPackageRequest,
        },
    },
};

#[cfg(target_arch = "wasm32")]
use xmtp_proto::xmtp::mls::api::v1::WelcomeMessage;

use xmtp_common::{ExponentialBackoff, Retry, RetryBuilder};
use xmtp_proto::api_client::XmtpTestClient;

pub fn exponential() -> RetryBuilder<ExponentialBackoff, ExponentialBackoff> {
    let e = ExponentialBackoff::default();
    Retry::builder().with_strategy(e.clone()).with_cooldown(e)
}

pub fn build_group_messages(num_messages: usize, group_id: Vec<u8>) -> Vec<GroupMessage> {
    let mut out: Vec<GroupMessage> = vec![];
    for i in 0..num_messages {
        out.push(GroupMessage {
            version: Some(GroupMessageVersion::V1(GroupMessageV1 {
                id: i as u64,
                created_ns: i as u64,
                group_id: group_id.clone(),
                data: vec![i as u8],
                sender_hmac: vec![],
            })),
        })
    }
    out
}

#[derive(thiserror::Error, Debug)]
pub enum MockError {
    #[error("MockQuery Error")]
    MockQuery,
    #[error("Mock Rate Limit")]
    RateLimit,
}

impl xmtp_proto::XmtpApiError for MockError {
    fn api_call(&self) -> Option<xmtp_proto::ApiEndpoint> {
        None
    }
    fn code(&self) -> Option<xmtp_proto::Code> {
        None
    }
    fn grpc_message(&self) -> Option<&str> {
        None
    }
}

impl xmtp_common::RetryableError for MockError {
    fn is_retryable(&self) -> bool {
        true
    }

    fn needs_cooldown(&self) -> bool {
        matches!(self, MockError::RateLimit)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use not_wasm::*;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

// Create a mock XmtpClient for testing the client wrapper
// need separate defs for wasm and not wasm, b/c `cfg_attr` not supportd in macro! block
#[cfg(not(target_arch = "wasm32"))]
mod not_wasm {
    use super::*;
    use xmtp_proto::xmtp::mls::api::v1::WelcomeMessage;

    mock! {
        pub ApiClient {}

        #[async_trait::async_trait]
        impl ClientWithMetadata for ApiClient {
            type Error = MockError;
            fn set_libxmtp_version(&mut self, version: String) -> Result<(), MockError>;
            fn set_app_version(&mut self, version: String) -> Result<(), MockError>;
        }

        #[async_trait::async_trait]
        impl XmtpMlsClient for ApiClient {
            type Error = MockError;
            async fn upload_key_package(&self, request: UploadKeyPackageRequest) -> Result<(), MockError>;
            async fn fetch_key_packages(
                &self,
                request: FetchKeyPackagesRequest,
            ) -> Result<FetchKeyPackagesResponse, MockError>;
            async fn send_group_messages(&self, request: SendGroupMessagesRequest) -> Result<(), MockError>;
            async fn send_welcome_messages(&self, request: SendWelcomeMessagesRequest) -> Result<(), MockError>;
            async fn query_group_messages(&self, request: QueryGroupMessagesRequest) -> Result<QueryGroupMessagesResponse, MockError>;
            async fn query_welcome_messages(&self, request: QueryWelcomeMessagesRequest) -> Result<QueryWelcomeMessagesResponse, MockError>;
        }

        #[async_trait::async_trait]
        impl XmtpMlsStreams for ApiClient {
            type Error = MockError;
            #[cfg(not(target_arch = "wasm32"))]
            type GroupMessageStream<'a> = futures::stream::BoxStream<'static, Result<GroupMessage, MockError>>;
            #[cfg(not(target_arch = "wasm32"))]
            type WelcomeMessageStream<'a> = futures::stream::BoxStream<'static, Result<WelcomeMessage, MockError>>;

            #[cfg(target_arch = "wasm32")]
            type GroupMessageStream<'a> = futures::stream::LocalBoxStream<'static, Result<GroupMessage, MockError>>;
            #[cfg(target_arch = "wasm32")]
            type WelcomeMessageStream<'a> = futures::stream::LocalBoxStream<'static, Result<WelcomeMessage, MockError>>;


            async fn subscribe_group_messages(&self, request: SubscribeGroupMessagesRequest) -> Result<<Self as XmtpMlsStreams>::GroupMessageStream<'static>, MockError>;
            async fn subscribe_welcome_messages(&self, request: SubscribeWelcomeMessagesRequest) -> Result<<Self as XmtpMlsStreams>::WelcomeMessageStream<'static>, MockError>;
        }

        #[async_trait::async_trait]
        impl XmtpIdentityClient for ApiClient {
            type Error = MockError;
            async fn publish_identity_update(&self, request: PublishIdentityUpdateRequest) -> Result<PublishIdentityUpdateResponse, MockError>;
            async fn get_identity_updates_v2(&self, request: GetIdentityUpdatesV2Request) -> Result<GetIdentityUpdatesV2Response, MockError>;
            async fn get_inbox_ids(&self, request: GetInboxIdsRequest) -> Result<GetInboxIdsResponse, MockError>;
            async fn verify_smart_contract_wallet_signatures(&self, request: VerifySmartContractWalletSignaturesRequest)
            -> Result<VerifySmartContractWalletSignaturesResponse, MockError>;
        }

        #[async_trait::async_trait]
        impl XmtpTestClient for ApiClient {
            async fn create_local() -> Self { ApiClient }
            async fn create_dev() -> Self { ApiClient }
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    mock! {
        pub ApiClient {}

        #[async_trait::async_trait(?Send)]
        impl ClientWithMetadata for ApiClient {
            type Error = MockError;
            fn set_libxmtp_version(&mut self, version: String) -> Result<(), MockError>;
            fn set_app_version(&mut self, version: String) -> Result<(), MockError>;
        }

        #[async_trait::async_trait(?Send)]
        impl XmtpMlsClient for ApiClient {
            type Error = MockError;
            async fn upload_key_package(&self, request: UploadKeyPackageRequest) -> Result<(), MockError>;
            async fn fetch_key_packages(
                &self,
                request: FetchKeyPackagesRequest,
            ) -> Result<FetchKeyPackagesResponse, MockError>;
            async fn send_group_messages(&self, request: SendGroupMessagesRequest) -> Result<(), MockError>;
            async fn send_welcome_messages(&self, request: SendWelcomeMessagesRequest) -> Result<(), MockError>;
            async fn query_group_messages(&self, request: QueryGroupMessagesRequest) -> Result<QueryGroupMessagesResponse, MockError>;
            async fn query_welcome_messages(&self, request: QueryWelcomeMessagesRequest) -> Result<QueryWelcomeMessagesResponse, MockError>;
        }

        #[async_trait::async_trait(?Send)]
        impl XmtpMlsStreams for ApiClient {
            type Error = MockError;
            #[cfg(not(target_arch = "wasm32"))]
            type GroupMessageStream<'a> = futures::stream::BoxStream<'static, Result<GroupMessage, MockError>>;
            #[cfg(not(target_arch = "wasm32"))]
            type WelcomeMessageStream<'a> = futures::stream::BoxStream<'static, Result<WelcomeMessage, MockError>>;

            #[cfg(target_arch = "wasm32")]
            type GroupMessageStream<'a> = futures::stream::LocalBoxStream<'static, Result<GroupMessage, MockError>>;
            #[cfg(target_arch = "wasm32")]
            type WelcomeMessageStream<'a> = futures::stream::LocalBoxStream<'static, Result<WelcomeMessage, MockError>>;


            async fn subscribe_group_messages(&self, request: SubscribeGroupMessagesRequest) -> Result<<Self as XmtpMlsStreams>::GroupMessageStream<'static>, MockError>;
            async fn subscribe_welcome_messages(&self, request: SubscribeWelcomeMessagesRequest) -> Result<<Self as XmtpMlsStreams>::WelcomeMessageStream<'static>, MockError>;
        }

        #[async_trait::async_trait(?Send)]
        impl XmtpIdentityClient for ApiClient {
            type Error = MockError;
            async fn publish_identity_update(&self, request: PublishIdentityUpdateRequest) -> Result<PublishIdentityUpdateResponse, MockError>;
            async fn get_identity_updates_v2(&self, request: GetIdentityUpdatesV2Request) -> Result<GetIdentityUpdatesV2Response, MockError>;
            async fn get_inbox_ids(&self, request: GetInboxIdsRequest) -> Result<GetInboxIdsResponse, MockError>;
            async fn verify_smart_contract_wallet_signatures(&self, request: VerifySmartContractWalletSignaturesRequest)
            -> Result<VerifySmartContractWalletSignaturesResponse, MockError>;
        }

        #[async_trait::async_trait(?Send)]
        impl XmtpTestClient for ApiClient {
            async fn create_local() -> Self { ApiClient }
            async fn create_dev() -> Self { ApiClient }
        }
    }
}
