use std::pin::Pin;

use crate::protocol::CursorStore;
use crate::protocol::XmtpQuery;
use futures::Stream;
use mockall::mock;
use std::sync::Arc;
use xmtp_proto::api::mock::MockApiBuilder;
use xmtp_proto::api_client::CursorAwareApi;
use xmtp_proto::api_client::XmtpTestClient;
use xmtp_proto::{
    api_client::{XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams},
    types::{GroupId, GroupMessage, GroupMessageMetadata, InstallationId, WelcomeMessage},
    xmtp::{
        identity::api::v1::{
            GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
            GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
            GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
            VerifySmartContractWalletSignaturesRequest,
            VerifySmartContractWalletSignaturesResponse,
        },
        mls::api::v1::{
            BatchPublishCommitLogRequest, BatchQueryCommitLogRequest, BatchQueryCommitLogResponse,
            FetchKeyPackagesRequest, FetchKeyPackagesResponse, SendGroupMessagesRequest,
            UploadKeyPackageRequest,
        },
    },
};
xmtp_common::if_native! {
    pub use not_wasm::*;
}

xmtp_common::if_wasm! {
    pub use wasm::*;
}

#[derive(thiserror::Error, Debug)]
pub enum MockError {
    #[error("MockQuery Error")]
    MockQuery,
    #[error("Mock Rate Limit")]
    RateLimit,
}

impl xmtp_common::RetryableError for MockError {
    fn is_retryable(&self) -> bool {
        true
    }
}

mock! {
    pub GroupStream { }
    impl Stream for GroupStream {
        type Item = Result<GroupMessage, MockError>;
        fn poll_next<'a>(self: Pin<&mut Self>, cx: &mut std::task::Context<'a> ) -> std::task::Poll<Option<Result<GroupMessage, MockError>> > ;
    }
}

mock! {
    pub WelcomeStream { }
    impl Stream for WelcomeStream {
        type Item = Result<xmtp_proto::types::WelcomeMessage, MockError>;
        fn poll_next<'a>(self: Pin<&mut Self>, cx: &mut std::task::Context<'a> ) -> std::task::Poll<Option<Result<xmtp_proto::types::WelcomeMessage, MockError>>> ;
    }
}

mock! {
    pub ApiClientWrapper {
        fn query_latest_group_message<Id: AsRef<[u8]> + Copy + 'static>(&self, group_id: Id) -> Result<Option<GroupMessage>, crate::ApiError>;
    }
}

// Create a mock XmtpClient for testing the client wrapper
// need separate defs for wasm and not wasm, b/c `cfg_attr` not supportd in macro! block
#[cfg(not(target_arch = "wasm32"))]
mod not_wasm {
    use super::*;

    #[derive(Clone)]
    pub struct ApiClient;

    mock! {
        pub ApiClient { }
        impl Clone for ApiClient {
            fn clone(&self) -> Self {
            }
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
            async fn send_welcome_messages(&self, request: xmtp_proto::mls_v1::SendWelcomeMessagesRequest) -> Result<(), MockError>;
            async fn query_group_messages(&self, group_id: GroupId) -> Result<Vec<GroupMessage>, MockError>;
            async fn query_latest_group_message(&self, group_id: GroupId) -> Result<Option<GroupMessage>, MockError>;

            async fn query_welcome_messages(&self, installation_key: InstallationId) -> Result<Vec<WelcomeMessage>, MockError>;
            async fn publish_commit_log(&self, request: BatchPublishCommitLogRequest) -> Result<(), MockError>;
            async fn query_commit_log(&self, request: BatchQueryCommitLogRequest) -> Result<BatchQueryCommitLogResponse, MockError>;
            async fn get_newest_group_message(&self, request: xmtp_proto::mls_v1::GetNewestGroupMessageRequest) -> Result<Vec<Option<GroupMessageMetadata>>, MockError>;
        }

        #[async_trait::async_trait]
        impl XmtpMlsStreams for ApiClient {
            type Error = MockError;
            type GroupMessageStream = MockGroupStream;
            type WelcomeMessageStream = MockWelcomeStream;
            #[mockall::concretize]
            async fn subscribe_group_messages(&self, group_ids: &[&GroupId]) -> Result<MockGroupStream, MockError>;
            #[mockall::concretize]
            async fn subscribe_group_messages_with_cursors(&self, groups_with_cursors: &[(&GroupId, xmtp_proto::types::GlobalCursor)]) -> Result<MockGroupStream, MockError>;
            #[mockall::concretize]
            async fn subscribe_welcome_messages(&self, installations: &[&InstallationId]) -> Result<MockWelcomeStream, MockError>;
        }

        #[async_trait::async_trait]
        impl XmtpIdentityClient for ApiClient {
            type Error = MockError;
            async fn publish_identity_update(&self, request: PublishIdentityUpdateRequest) -> Result<PublishIdentityUpdateResponse, MockError>;
            async fn get_identity_updates_v2(&self, request: GetIdentityUpdatesV2Request) -> Result<GetIdentityUpdatesV2Response, MockError>;
            async fn get_inbox_ids(&self, request: GetInboxIdsRequest) -> Result<GetInboxIdsResponse, MockError>;
            async fn verify_smart_contract_wallet_signatures(&self, request: VerifySmartContractWalletSignaturesRequest) -> Result<VerifySmartContractWalletSignaturesResponse, MockError>;
        }

        impl XmtpTestClient for ApiClient {
            type Builder = MockApiBuilder;
            fn create_local() -> MockApiBuilder { MockApiBuilder }
            fn create_dev() -> MockApiBuilder { MockApiBuilder }
            fn create_d14n() -> MockApiBuilder { MockApiBuilder }
            fn create_gateway() -> MockApiBuilder { MockApiBuilder }
        }

        impl CursorAwareApi for ApiClient {
            type CursorStore = Arc<dyn CursorStore>;
            fn set_cursor_store(&self, store: <Self as CursorAwareApi>::CursorStore);
        }
    }

    #[async_trait::async_trait]
    impl XmtpQuery for MockApiClient {
        type Error = MockError;
        async fn query_at(
            &self,
            _topic: xmtp_proto::types::Topic,
            _at: Option<xmtp_proto::types::GlobalCursor>,
        ) -> Result<crate::protocol::XmtpEnvelope, Self::Error> {
            panic!("query at cannot yet be mocked")
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    #[derive(Clone)]
    pub struct ApiClient;

    mock! {
        pub ApiClient {}

        impl Clone for ApiClient {
            fn clone(&self) -> Self;
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
            async fn send_welcome_messages(&self, request: xmtp_proto::mls_v1::SendWelcomeMessagesRequest) -> Result<(), MockError>;
            async fn query_group_messages(&self, group_id: GroupId) -> Result<Vec<GroupMessage>, MockError>;
            async fn query_latest_group_message(&self, group_id: GroupId) -> Result<Option<GroupMessage>, MockError>;
            async fn query_welcome_messages(&self, installation_key: InstallationId) -> Result<Vec<WelcomeMessage>, MockError>;
            async fn publish_commit_log(&self, request: BatchPublishCommitLogRequest) -> Result<(), MockError>;
            async fn query_commit_log(&self, request: BatchQueryCommitLogRequest) -> Result<BatchQueryCommitLogResponse, MockError>;
            async fn get_newest_group_message(&self, request: xmtp_proto::mls_v1::GetNewestGroupMessageRequest) -> Result<Vec<Option<GroupMessageMetadata>>, MockError>;
        }

        #[async_trait::async_trait(?Send)]
        impl XmtpMlsStreams for ApiClient {
            type Error = MockError;
            type GroupMessageStream = MockGroupStream;
            type WelcomeMessageStream = MockWelcomeStream;

            #[mockall::concretize]
            async fn subscribe_group_messages(&self, group_ids: &[&GroupId]) -> Result<MockGroupStream, MockError>;
            #[mockall::concretize]
            async fn subscribe_group_messages_with_cursors(&self, groups_with_cursors: &[(&GroupId, xmtp_proto::types::GlobalCursor)]) -> Result<MockGroupStream, MockError>;
            #[mockall::concretize]
            async fn subscribe_welcome_messages(&self, installations: &[&InstallationId]) -> Result<MockWelcomeStream, MockError>;
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
            type Builder = MockApiBuilder;
            fn create_local() -> MockApiBuilder { MockApiBuilder }
            fn create_dev() -> MockApiBuilder { MockApiBuilder }
            fn create_d14n() -> MockApiBuilder { MockApiBuilder }
            fn create_gateway() -> MockApiBuilder { MockApiBuilder }
        }


        impl CursorAwareApi for ApiClient {
            type CursorStore = Arc<dyn CursorStore>;
            fn set_cursor_store(&self, store: <Self as CursorAwareApi>::CursorStore);
        }
    }

    #[async_trait::async_trait(?Send)]
    impl XmtpQuery for MockApiClient {
        type Error = MockError;
        async fn query_at(
            &self,
            _topic: xmtp_proto::types::Topic,
            _at: Option<xmtp_proto::types::GlobalCursor>,
        ) -> Result<crate::protocol::XmtpEnvelope, Self::Error> {
            panic!("query at cannot yet be mocked")
        }
    }
}
