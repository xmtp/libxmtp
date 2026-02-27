use crate::MigrationClient;
use crate::protocol::CursorStore;

use xmtp_proto::api::{ApiClientError, Client};
use xmtp_proto::api_client::{BoxedGroupS, BoxedWelcomeS, XmtpMlsStreams};
use xmtp_proto::types::{GroupId, InstallationId, TopicCursor};

#[xmtp_common::async_trait]
impl<V3, D14n, Store> XmtpMlsStreams for MigrationClient<V3, D14n, Store>
where
    V3: Client,
    D14n: Client,
    Store: CursorStore + Clone,
{
    type Error = ApiClientError;

    type GroupMessageStream = BoxedGroupS<ApiClientError>;

    type WelcomeMessageStream = BoxedWelcomeS<ApiClientError>;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        Ok(self
            .choose_client()
            .await?
            .subscribe_group_messages(group_ids)
            .await?)
    }

    async fn subscribe_group_messages_with_cursors(
        &self,
        topics: &TopicCursor,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        Ok(self
            .choose_client()
            .await?
            .subscribe_group_messages_with_cursors(topics)
            .await?)
    }

    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        Ok(self
            .choose_client()
            .await?
            .subscribe_welcome_messages(installations)
            .await?)
    }
}
