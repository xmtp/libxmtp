use super::D14nClient;
use crate::d14n::GetNewestEnvelopes;
use crate::d14n::PublishClientEnvelopes;
use crate::d14n::QueryEnvelope;
use crate::protocol::CollectionExtractor;
use crate::protocol::GroupMessageExtractor;
use crate::protocol::KeyPackagesExtractor;
use crate::protocol::SequencedExtractor;
use crate::protocol::TopicKind;
use crate::protocol::WelcomeMessageExtractor;
use crate::protocol::traits::Envelope;
use crate::protocol::traits::EnvelopeCollection;
use crate::protocol::traits::Extractor;
use xmtp_common::RetryableError;
use xmtp_proto::api_client::{ApiStats, XmtpMlsClient};
use xmtp_proto::mls_v1;
use xmtp_proto::traits::Client;
use xmtp_proto::traits::{ApiClientError, Query};
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::message_api::GetNewestEnvelopeResponse;
use xmtp_proto::xmtp::xmtpv4::message_api::QueryEnvelopesResponse;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, P, E> XmtpMlsClient for D14nClient<C, P>
where
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    P: Send + Sync + Client,
    C: Send + Sync + Client<Error = E>,
    ApiClientError<E>: From<ApiClientError<<P as xmtp_proto::traits::Client>::Error>>,
{
    type Error = ApiClientError<E>;

    #[tracing::instrument(level = "trace", skip_all)]
    async fn upload_key_package(
        &self,
        request: mls_v1::UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        let envelopes = request.client_envelope()?;
        PublishClientEnvelopes::builder()
            .envelope(envelopes)
            .build()?
            .query(&self.payer_client)
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

        PublishClientEnvelopes::builder()
            .envelopes(envelopes)
            .build()?
            .query(&self.payer_client)
            .await?;

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn send_welcome_messages(
        &self,
        request: mls_v1::SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        let envelope = request.messages.client_envelopes()?;

        PublishClientEnvelopes::builder()
            .envelopes(envelope)
            .build()?
            .query(&self.payer_client)
            .await?;

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn query_group_messages(
        &self,
        request: mls_v1::QueryGroupMessagesRequest,
    ) -> Result<mls_v1::QueryGroupMessagesResponse, Self::Error> {
        let response: QueryEnvelopesResponse = QueryEnvelope::builder()
            .topic(TopicKind::GroupMessagesV1.build(request.group_id.as_slice()))
            .paging_info(request.paging_info)
            .build()?
            .query(&self.message_client)
            .await?;

        let messages = SequencedExtractor::builder()
            .envelopes(response.envelopes)
            .build::<GroupMessageExtractor>()
            .get()?;

        Ok(mls_v1::QueryGroupMessagesResponse {
            messages,
            paging_info: None,
        })
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn query_welcome_messages(
        &self,
        request: mls_v1::QueryWelcomeMessagesRequest,
    ) -> Result<mls_v1::QueryWelcomeMessagesResponse, Self::Error> {
        let topic = TopicKind::WelcomeMessagesV1.build(request.installation_key.as_slice());

        let response = QueryEnvelope::builder()
            .topic(topic)
            .paging_info(request.paging_info)
            .build()?
            .query(&self.message_client)
            .await?;

        let messages = SequencedExtractor::builder()
            .envelopes(response.envelopes)
            .build::<WelcomeMessageExtractor>()
            .get()?;

        Ok(mls_v1::QueryWelcomeMessagesResponse {
            messages,
            paging_info: None,
        })
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

    fn stats(&self) -> ApiStats {
        Default::default()
    }
}
