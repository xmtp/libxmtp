use mockall::mock;
use xmtp_proto::{
    api_client::{ClientWithMetadata, Error, XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams},
    xmtp::{
        identity::api::v1::{
            GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
            GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
            GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
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

#[cfg(feature = "http-api")]
use xmtp_proto::xmtp::mls::api::v1::WelcomeMessage;

use crate::XmtpTestClient;

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

// Create a mock XmtpClient for testing the client wrapper
mock! {
    pub ApiClient {}

    impl ClientWithMetadata for ApiClient {
        fn set_libxmtp_version(&mut self, version: String) -> Result<(), Error>;
        fn set_app_version(&mut self, version: String) -> Result<(), Error>;
    }

    impl XmtpMlsClient for ApiClient {
        async fn upload_key_package(&self, request: UploadKeyPackageRequest) -> Result<(), Error>;
        async fn fetch_key_packages(
            &self,
            request: FetchKeyPackagesRequest,
        ) -> Result<FetchKeyPackagesResponse, Error>;
        async fn send_group_messages(&self, request: SendGroupMessagesRequest) -> Result<(), Error>;
        async fn send_welcome_messages(&self, request: SendWelcomeMessagesRequest) -> Result<(), Error>;
        async fn query_group_messages(&self, request: QueryGroupMessagesRequest) -> Result<QueryGroupMessagesResponse, Error>;
        async fn query_welcome_messages(&self, request: QueryWelcomeMessagesRequest) -> Result<QueryWelcomeMessagesResponse, Error>;
    }

    impl XmtpMlsStreams for ApiClient {
        #[cfg(not(feature = "http-api"))]
        type GroupMessageStream<'a> = xmtp_api_grpc::GroupMessageStream;
        #[cfg(not(feature = "http-api"))]
        type WelcomeMessageStream<'a> = xmtp_api_grpc::WelcomeMessageStream;

        #[cfg(feature = "http-api")]
        type GroupMessageStream<'a> = futures::stream::BoxStream<'static, Result<GroupMessage, Error>>;
        #[cfg(feature = "http-api")]
        type WelcomeMessageStream<'a> = futures::stream::BoxStream<'static, Result<WelcomeMessage, Error>>;


        async fn subscribe_group_messages(&self, request: SubscribeGroupMessagesRequest) -> Result<<Self as XmtpMlsStreams>::GroupMessageStream<'static>, Error>;
        async fn subscribe_welcome_messages(&self, request: SubscribeWelcomeMessagesRequest) -> Result<<Self as XmtpMlsStreams>::WelcomeMessageStream<'static>, Error>;
    }

    impl XmtpIdentityClient for ApiClient {
        async fn publish_identity_update(&self, request: PublishIdentityUpdateRequest) -> Result<PublishIdentityUpdateResponse, Error>;
        async fn get_identity_updates_v2(&self, request: GetIdentityUpdatesV2Request) -> Result<GetIdentityUpdatesV2Response, Error>;
        async fn get_inbox_ids(&self, request: GetInboxIdsRequest) -> Result<GetInboxIdsResponse, Error>;
    }

    impl XmtpTestClient for ApiClient {
        async fn create_local() -> Self { ApiClient }
        async fn create_dev() -> Self { ApiClient }
    }
}
