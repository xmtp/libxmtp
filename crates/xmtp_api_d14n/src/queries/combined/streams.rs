use crate::CombinedD14nClient;
use crate::protocol::CursorStore;

use futures::StreamExt;
use xmtp_api_grpc::error::GrpcError;
use xmtp_proto::api::{ApiClientError, Client};
use xmtp_proto::api_client::{BoxedGroupS, BoxedWelcomeS, XmtpMlsStreams};
use xmtp_proto::types::{GroupId, InstallationId, TopicCursor};

#[xmtp_common::async_trait]
impl<C, Store> XmtpMlsStreams for CombinedD14nClient<C, Store>
where
    C: Client<Error = GrpcError>,
    <C as Client>::Stream: 'static,
    Store: CursorStore + Clone,
{
    type Error = ApiClientError<GrpcError>;

    type GroupMessageStream = BoxedGroupS<ApiClientError<GrpcError>>;

    type WelcomeMessageStream = BoxedWelcomeS<ApiClientError<GrpcError>>;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        Ok(self
            .choose_client()
            .await?
            .subscribe_group_messages(group_ids)
            .await?
            .boxed())
    }

    async fn subscribe_group_messages_with_cursors(
        &self,
        topics: &TopicCursor,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        Ok(self
            .choose_client()
            .await?
            .subscribe_group_messages_with_cursors(topics)
            .await?
            .boxed())
    }

    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        Ok(self
            .choose_client()
            .await?
            .subscribe_welcome_messages(installations)
            .await?
            .boxed())
    }
}
