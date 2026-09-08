use xmtp_proto::{
    api::ApiClientError,
    api_client::{BoxedGroupS, BoxedWelcomeS, XmtpBackendClient, XmtpMlsStreams},
    backend_v1::*,
    types::{GroupId, InstallationId, TopicCursor},
};
mockall::mock! {
 pub BackendClient {}
 #[xmtp_common::async_trait]
 impl XmtpBackendClient for BackendClient {
 type Error = ApiClientError;
async fn publish(&self, request: PublishRequest) -> Result<PublishResponse, ApiClientError>;
async fn query(&self, request: QueryRequest) -> Result<QueryResponse, ApiClientError>;
async fn query_newest(&self, request: QueryNewestRequest) -> Result<QueryNewestResponse, ApiClientError>;
async fn get(&self, request: GetRequest) -> Result<ServerEnvelope, ApiClientError>;
async fn get_inbox_ids(&self, request: GetInboxIdsRequest) -> Result<GetInboxIdsResponse, ApiClientError>;
async fn verify_smart_contract_wallet_signatures(&self, request: VerifySmartContractWalletSignaturesRequest) -> Result<VerifySmartContractWalletSignaturesResponse, ApiClientError>;
}
#[xmtp_common::async_trait]
impl XmtpMlsStreams for BackendClient {
type Error = ApiClientError;
type GroupMessageStream = BoxedGroupS<ApiClientError>;
type WelcomeMessageStream = BoxedWelcomeS<ApiClientError>;
#[mockall::concretize]
async fn subscribe_group_messages(&self, input: &[&GroupId]) -> Result<BoxedGroupS<ApiClientError>, ApiClientError>;
#[mockall::concretize]
async fn subscribe_group_messages_with_cursors(&self, input: &TopicCursor) -> Result<BoxedGroupS<ApiClientError>, ApiClientError>;
#[mockall::concretize]
async fn subscribe_welcome_messages(&self, input: &[&InstallationId]) -> Result<BoxedWelcomeS<ApiClientError>, ApiClientError>;
#[mockall::concretize]
async fn subscribe_welcome_messages_with_cursors(&self, input: &TopicCursor) -> Result<BoxedWelcomeS<ApiClientError>, ApiClientError>;
}
}
