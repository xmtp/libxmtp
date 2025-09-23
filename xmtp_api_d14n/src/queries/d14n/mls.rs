use super::D14nClient;
use crate::d14n::GetNewestEnvelopes;
use crate::d14n::PublishClientEnvelopes;
use crate::d14n::QueryEnvelope;
use crate::protocol::CollectionExtractor;
use crate::protocol::EnvelopeError;
use crate::protocol::GroupMessageExtractor;
use crate::protocol::KeyPackagesExtractor;
use crate::protocol::ProtocolEnvelope;
use crate::protocol::SequencedExtractor;
use crate::protocol::TopicKind;
use crate::protocol::WelcomeMessageExtractor;
use crate::protocol::traits::Envelope;
use crate::protocol::traits::EnvelopeCollection;
use crate::protocol::traits::Extractor;
use xmtp_common::RetryableError;
use xmtp_configuration::MAX_PAGE_SIZE;
use xmtp_proto::api;
use xmtp_proto::api::Client;
use xmtp_proto::api::{ApiClientError, Query};
use xmtp_proto::api_client::XmtpMlsClient;
use xmtp_proto::mls_v1;
use xmtp_proto::types::GroupId;
use xmtp_proto::types::InstallationId;
use xmtp_proto::types::WelcomeMessage;
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::message_api::GetNewestEnvelopeResponse;
use xmtp_proto::xmtp::xmtpv4::message_api::QueryEnvelopesResponse;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, G, E> XmtpMlsClient for D14nClient<C, G>
where
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    G: Send + Sync + Client,
    C: Send + Sync + Client<Error = E>,
    ApiClientError<E>: From<ApiClientError<<G as xmtp_proto::api::Client>::Error>> + 'static,
{
    type Error = ApiClientError<E>;

    #[tracing::instrument(level = "trace", skip_all)]
    async fn upload_key_package(
        &self,
        request: mls_v1::UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        let envelopes = request.client_envelope()?;
        api::ignore(
            PublishClientEnvelopes::builder()
                .envelope(envelopes)
                .build()?,
        )
        .query(&self.gateway_client)
        .await?;

        Ok::<_, Self::Error>(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn fetch_key_packages(
        &self,
        request: mls_v1::FetchKeyPackagesRequest,
    ) -> Result<mls_v1::FetchKeyPackagesResponse, Self::Error> {
        let topics = request
            .installation_keys
            .iter()
            .map(|key| TopicKind::KeyPackagesV1.build(key))
            .collect();

        let result: GetNewestEnvelopeResponse = GetNewestEnvelopes::builder()
            .topics(topics)
            .build()?
            .query(&self.message_client)
            .await?;
        let extractor = CollectionExtractor::new(result.results, KeyPackagesExtractor::new());
        let key_packages = extractor.get()?;
        Ok(mls_v1::FetchKeyPackagesResponse { key_packages })
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn send_group_messages(
        &self,
        request: mls_v1::SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        let envelopes: Vec<ClientEnvelope> = request.messages.client_envelopes()?;

        api::ignore(
            PublishClientEnvelopes::builder()
                .envelopes(envelopes)
                .build()?,
        )
        .query(&self.gateway_client)
        .await?;

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn send_welcome_messages(
        &self,
        request: mls_v1::SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        let envelope = request.messages.client_envelopes()?;

        api::ignore(
            PublishClientEnvelopes::builder()
                .envelopes(envelope)
                .build()?,
        )
        .query(&self.gateway_client)
        .await?;

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn query_group_messages(
        &self,
        group_id: GroupId,
        cursor: Vec<xmtp_proto::types::Cursor>,
    ) -> Result<Vec<xmtp_proto::types::GroupMessage>, Self::Error> {
        let response: QueryEnvelopesResponse = QueryEnvelope::builder()
            .topic(TopicKind::GroupMessagesV1.build(group_id))
            .last_seen(cursor)
            .limit(MAX_PAGE_SIZE)
            .build()?
            .query(&self.message_client)
            .await?;

        let messages = SequencedExtractor::builder()
            .envelopes(response.envelopes)
            .build::<GroupMessageExtractor>()
            .get()?;
        Ok(messages
            .into_iter()
            .map(|i| i.map_err(EnvelopeError::from))
            .collect::<Result<_, _>>()?)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn query_latest_group_message(
        &self,
        group_id: GroupId,
    ) -> Result<Option<xmtp_proto::types::GroupMessage>, Self::Error> {
        let response: GetNewestEnvelopeResponse = GetNewestEnvelopes::builder()
            .topic(TopicKind::GroupMessagesV1.build(group_id))
            .build()?
            .query(&self.message_client)
            .await?;
        // expect at most a single message
        let mut extractor = GroupMessageExtractor::default();
        if response.results.is_empty() {
            return Ok(None);
        }
        response
            .results
            .into_iter()
            .next()
            .as_ref()
            .accept(&mut extractor)?;
        Ok(Some(extractor.get().map_err(EnvelopeError::from)?))
    }

    #[tracing::instrument(level = "info", skip(self))]
    async fn query_welcome_messages(
        &self,
        installation_key: InstallationId,
        cursor: Vec<xmtp_proto::types::Cursor>,
    ) -> Result<Vec<WelcomeMessage>, Self::Error> {
        let topic = TopicKind::WelcomeMessagesV1.build(installation_key.as_slice());

        let response: QueryEnvelopesResponse = QueryEnvelope::builder()
            .topic(topic)
            .last_seen(cursor)
            .limit(MAX_PAGE_SIZE)
            .build()?
            .query(&self.message_client)
            .await?;

        let messages = SequencedExtractor::builder()
            .envelopes(response.envelopes)
            .build::<WelcomeMessageExtractor>()
            .get()?;
        Ok(messages
            .into_iter()
            .map(|i| i.map_err(EnvelopeError::from))
            .collect::<Result<_, _>>()?)
    }

    // TODO(cvoell): implement
    #[tracing::instrument(level = "debug", skip_all)]
    async fn publish_commit_log(
        &self,
        _request: mls_v1::BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    // TODO(cvoell): implement
    async fn query_commit_log(
        &self,
        _request: mls_v1::BatchQueryCommitLogRequest,
    ) -> Result<mls_v1::BatchQueryCommitLogResponse, Self::Error> {
        Ok(mls_v1::BatchQueryCommitLogResponse { responses: vec![] })
    }
}
