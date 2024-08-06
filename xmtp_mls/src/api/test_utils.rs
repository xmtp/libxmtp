use async_trait::async_trait;
use mockall::mock;
use xmtp_proto::{
    api_client::{
        Error, GroupMessageStream, WelcomeMessageStream, XmtpIdentityClient, XmtpMlsClient,
    },
    xmtp::identity::api::v1::{
        GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
        GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
        GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
    },
    xmtp::mls::api::v1::{
        group_message::{Version as GroupMessageVersion, V1 as GroupMessageV1},
        FetchKeyPackagesRequest, FetchKeyPackagesResponse, GetIdentityUpdatesRequest,
        GetIdentityUpdatesResponse, GroupMessage, QueryGroupMessagesRequest,
        QueryGroupMessagesResponse, QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse,
        RegisterInstallationRequest, RegisterInstallationResponse, SendGroupMessagesRequest,
        SendWelcomeMessagesRequest, SubscribeGroupMessagesRequest, SubscribeWelcomeMessagesRequest,
        UploadKeyPackageRequest,
    },
};

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

    #[async_trait]
    impl XmtpMlsClient for ApiClient {
        async fn register_installation(
            &self,
            request: RegisterInstallationRequest,
        ) -> Result<RegisterInstallationResponse, Error>;
        async fn upload_key_package(&self, request: UploadKeyPackageRequest) -> Result<(), Error>;
        async fn fetch_key_packages(
            &self,
            request: FetchKeyPackagesRequest,
        ) -> Result<FetchKeyPackagesResponse, Error>;
        async fn send_group_messages(&self, request: SendGroupMessagesRequest) -> Result<(), Error>;
        async fn send_welcome_messages(&self, request: SendWelcomeMessagesRequest) -> Result<(), Error>;
        async fn get_identity_updates(
            &self,
            request: GetIdentityUpdatesRequest,
        ) -> Result<GetIdentityUpdatesResponse, Error>;
        async fn query_group_messages(&self, request: QueryGroupMessagesRequest) -> Result<QueryGroupMessagesResponse, Error>;
        async fn query_welcome_messages(&self, request: QueryWelcomeMessagesRequest) -> Result<QueryWelcomeMessagesResponse, Error>;
        async fn subscribe_group_messages(&self, request: SubscribeGroupMessagesRequest) -> Result<GroupMessageStream, Error>;
        async fn subscribe_welcome_messages(&self, request: SubscribeWelcomeMessagesRequest) -> Result<WelcomeMessageStream, Error>;
    }

    #[async_trait]
    impl XmtpIdentityClient for ApiClient {
        async fn publish_identity_update(&self, request: PublishIdentityUpdateRequest) -> Result<PublishIdentityUpdateResponse, Error>;
        async fn get_identity_updates_v2(&self, request: GetIdentityUpdatesV2Request) -> Result<GetIdentityUpdatesV2Response, Error>;
        async fn get_inbox_ids(&self, request: GetInboxIdsRequest) -> Result<GetInboxIdsResponse, Error>;
    }

    #[async_trait]
    impl XmtpTestClient for ApiClient {
        async fn create_local() -> Self { ApiClient }
        async fn create_dev() -> Self { ApiClient }
    }
}
