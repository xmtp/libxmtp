use crate::protocol::{ProtocolEnvelope, V3WelcomeMessageExtractor};
use crate::protocol::{SequencedExtractor, V3GroupMessageExtractor, traits::Extractor};
use crate::{V3Client, v3::*};
use xmtp_common::RetryableError;
use xmtp_configuration::MAX_PAGE_SIZE;
use xmtp_proto::api::{self, ApiClientError, Client, Query};
use xmtp_proto::api_client::XmtpMlsClient;
use xmtp_proto::mls_v1::{self, GroupMessage as ProtoGroupMessage, PagingInfo, SortDirection};
use xmtp_proto::types::{Cursor, GroupId, InstallationId, WelcomeMessage};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, E> XmtpMlsClient for V3Client<C>
where
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    C: Send + Sync + Client<Error = E>,
    ApiClientError<E>: From<ApiClientError<<C as Client>::Error>> + Send + Sync + 'static,
{
    type Error = ApiClientError<E>;

    async fn upload_key_package(
        &self,
        request: mls_v1::UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        UploadKeyPackage::builder()
            .key_package(request.key_package)
            .is_inbox_id_credential(request.is_inbox_id_credential)
            .build()?
            .query(&self.client)
            .await
    }
    async fn fetch_key_packages(
        &self,
        request: mls_v1::FetchKeyPackagesRequest,
    ) -> Result<mls_v1::FetchKeyPackagesResponse, Self::Error> {
        FetchKeyPackages::builder()
            .installation_keys(request.installation_keys)
            .build()?
            .query(&self.client)
            .await
    }
    async fn send_group_messages(
        &self,
        request: mls_v1::SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        SendGroupMessages::builder()
            .messages(request.messages)
            .build()?
            .query(&self.client)
            .await
    }
    async fn send_welcome_messages(
        &self,
        request: mls_v1::SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        SendWelcomeMessages::builder()
            .messages(request.messages)
            .build()?
            .query(&self.client)
            .await
    }
    async fn query_group_messages(
        &self,
        group_id: GroupId,
        cursors: Vec<xmtp_proto::types::Cursor>,
    ) -> Result<Vec<xmtp_proto::types::GroupMessage>, Self::Error> {
        let cursor = cursors.into_iter().map(|c| c.sequence_id).max();
        let endpoint = QueryGroupMessages::builder()
            .group_id(group_id.to_vec())
            .paging_info(PagingInfo {
                limit: MAX_PAGE_SIZE,
                direction: SortDirection::Ascending as i32,
                id_cursor: cursor.unwrap_or(0),
            })
            .build()?;
        let messages = api::v3_paged(api::retry(endpoint), cursor)
            .query(&self.client)
            .await?;
        let messages = SequencedExtractor::builder()
            .envelopes(messages)
            .build::<V3GroupMessageExtractor>()
            .get()?;
        Ok(messages
            .into_iter()
            .collect::<Result<Vec<Option<_>>, _>>()?
            .into_iter()
            .flatten()
            .collect())
    }

    async fn query_latest_group_message(
        &self,
        group_id: GroupId,
    ) -> Result<Option<xmtp_proto::types::GroupMessage>, Self::Error> {
        let endpoint = QueryGroupMessages::builder()
            .group_id(group_id.to_vec())
            .paging_info(PagingInfo {
                limit: 1,
                direction: SortDirection::Descending as i32,
                id_cursor: 0,
            })
            .build()?;
        let message: Option<ProtoGroupMessage> = api::retry(endpoint)
            .query(&self.client)
            .await?
            .messages
            .into_iter()
            .next();
        let mut extractor = V3GroupMessageExtractor::default();
        message.as_ref().accept(&mut extractor)?;
        Ok(extractor.get()?)
    }

    async fn query_welcome_messages(
        &self,
        installation_key: InstallationId,
        cursors: Vec<Cursor>,
    ) -> Result<Vec<WelcomeMessage>, Self::Error> {
        let cursor = cursors.into_iter().map(|c| c.sequence_id).max();
        let endpoint = QueryWelcomeMessages::builder()
            .installation_key(installation_key)
            .paging_info(PagingInfo {
                limit: MAX_PAGE_SIZE,
                direction: SortDirection::Ascending as i32,
                id_cursor: cursor.unwrap_or(0),
            })
            .build()?;
        let messages = api::v3_paged(api::retry(endpoint), Some(cursor.unwrap_or(0)))
            .query(&self.client)
            .await?;
        let messages = SequencedExtractor::builder()
            .envelopes(messages)
            .build::<V3WelcomeMessageExtractor>()
            .get()?;
        Ok(messages.into_iter().collect::<Result<_, _>>()?)
    }

    async fn publish_commit_log(
        &self,
        request: mls_v1::BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        PublishCommitLog::builder()
            .commit_log_entries(request.requests)
            .build()?
            .query(&self.client)
            .await
    }

    async fn query_commit_log(
        &self,
        request: mls_v1::BatchQueryCommitLogRequest,
    ) -> Result<mls_v1::BatchQueryCommitLogResponse, Self::Error> {
        QueryCommitLog::builder()
            .query_log_requests(request.requests)
            .build()?
            .query(&self.client)
            .await
    }
}
