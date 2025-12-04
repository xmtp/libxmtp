use super::D14nClient;
use crate::d14n::GetNewestEnvelopes;
use crate::d14n::PublishClientEnvelopes;
use crate::d14n::QueryEnvelope;
use crate::protocol::CollectionExtractor;
use crate::protocol::CursorStore;
use crate::protocol::EnvelopeError;
use crate::protocol::GroupMessageExtractor;
use crate::protocol::KeyPackagesExtractor;
use crate::protocol::MessageMetadataExtractor;
use crate::protocol::ProtocolEnvelope;
use crate::protocol::SequencedExtractor;
use crate::protocol::WelcomeMessageExtractor;
use crate::protocol::resolve;
use crate::protocol::traits::Envelope;
use crate::protocol::traits::EnvelopeCollection;
use crate::protocol::traits::Extractor;
use crate::queries::D14nCombinatorExt;
use xmtp_common::RetryableError;
use xmtp_configuration::MAX_PAGE_SIZE;
use xmtp_proto::api;
use xmtp_proto::api::Client;
use xmtp_proto::api::EndpointExt;
use xmtp_proto::api::{ApiClientError, Query};
use xmtp_proto::api_client::XmtpMlsClient;
use xmtp_proto::mls_v1;
use xmtp_proto::mls_v1::BatchQueryCommitLogResponse;
use xmtp_proto::types::GroupId;
use xmtp_proto::types::GroupMessageMetadata;
use xmtp_proto::types::InstallationId;
use xmtp_proto::types::Topic;
use xmtp_proto::types::TopicCursor;
use xmtp_proto::types::TopicKind;
use xmtp_proto::types::WelcomeMessage;
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::message_api::GetNewestEnvelopeResponse;

#[xmtp_common::async_trait]
impl<C, Store, E> XmtpMlsClient for D14nClient<C, Store>
where
    E: RetryableError + 'static,
    C: Client<Error = E>,
    ApiClientError<E>: From<ApiClientError<<C as xmtp_proto::api::Client>::Error>> + 'static,
    Store: CursorStore,
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
        .query(&self.client)
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
            .map(Topic::new_key_package)
            .map(Into::into)
            .collect();

        let result: GetNewestEnvelopeResponse = GetNewestEnvelopes::builder()
            .topics(topics)
            .build()?
            .query(&self.client)
            .await?;
        tracing::info!("got {} envelopes", result.results.len());
        let extractor = CollectionExtractor::new(result.results, KeyPackagesExtractor::new());
        let key_packages = extractor.get()?;
        Ok(mls_v1::FetchKeyPackagesResponse { key_packages })
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn send_group_messages(
        &self,
        request: mls_v1::SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        let hashes = request.messages.sha256_hashes()?;
        let mut dependencies = self.cursor_store.find_message_dependencies(
            hashes
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .as_slice(),
        )?;
        let mut envelopes: Vec<ClientEnvelope> = request.messages.client_envelopes()?;
        envelopes.iter_mut().try_for_each(|envelope| {
            let data = envelope.sha256_hash()?;
            let dependency = dependencies.remove(&data);
            let mut aad = envelope.aad.clone().unwrap_or_default();
            aad.depends_on = dependency.map(Into::into);
            envelope.aad = Some(aad);
            Ok(())
        })?;

        PublishClientEnvelopes::builder()
            .envelopes(envelopes)
            .build()?
            .ignore_response()
            .query(&self.client)
            .await?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn send_welcome_messages(
        &self,
        request: mls_v1::SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        let envelopes = request.messages.client_envelopes()?;

        api::ignore(
            PublishClientEnvelopes::builder()
                .envelopes(envelopes)
                .build()?,
        )
        .query(&self.client)
        .await?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn query_group_messages(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<xmtp_proto::types::GroupMessage>, Self::Error> {
        let topic = TopicKind::GroupMessagesV1.create(&group_id);
        let lcc = self.cursor_store.lowest_common_cursor(&[&topic])?;
        let mut topic_cursor = TopicCursor::default();
        topic_cursor.insert(topic.clone(), lcc.clone());
        let resolver = resolve::network_backoff(&self.client);
        let response = QueryEnvelope::builder()
            .topic(topic)
            .last_seen(lcc)
            .limit(MAX_PAGE_SIZE)
            .build()?
            .ordered(resolver, topic_cursor)
            .query(&self.client)
            .await?;

        let messages = SequencedExtractor::builder()
            .envelopes(response)
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
            .topic(Topic::new_group_message(group_id))
            .build()?
            .query(&self.client)
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
    ) -> Result<Vec<WelcomeMessage>, Self::Error> {
        let topic = TopicKind::WelcomeMessagesV1.create(installation_key);
        let lcc = self.cursor_store.lowest_common_cursor(&[&topic])?;
        tracing::info!("querying welcomes @{:?}", lcc);
        let response = QueryEnvelope::builder()
            .topic(topic)
            .last_seen(lcc)
            .limit(MAX_PAGE_SIZE)
            .build()?
            .query(&self.client)
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

    #[tracing::instrument(level = "debug", skip_all)]
    async fn publish_commit_log(
        &self,
        _request: mls_v1::BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn query_commit_log(
        &self,
        _request: mls_v1::BatchQueryCommitLogRequest,
    ) -> Result<mls_v1::BatchQueryCommitLogResponse, Self::Error> {
        tracing::debug!("commit log disabled for d14n");
        Ok(BatchQueryCommitLogResponse { responses: vec![] })
    }

    async fn get_newest_group_message(
        &self,
        request: mls_v1::GetNewestGroupMessageRequest,
    ) -> Result<Vec<Option<GroupMessageMetadata>>, Self::Error> {
        let topics: Vec<Vec<u8>> = request
            .group_ids
            .into_iter()
            .map(Topic::new_group_message)
            .map(Into::into)
            .collect();

        let response = GetNewestEnvelopes::builder()
            .topics(topics)
            .build()?
            .query(&self.client)
            .await?;

        let extractor = CollectionExtractor::new(response.results, MessageMetadataExtractor::new());
        let responses = extractor.get()?;

        Ok(responses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        protocol::traits::{EnvelopeVisitor, Extractor},
        queries::d14n::test::{TestCursorStore, group_message_request},
    };
    use futures::FutureExt;
    use proptest::prelude::*;
    use prost::Message;
    use xmtp_proto::xmtp::xmtpv4::message_api::get_newest_envelope_response;
    use xmtp_proto::xmtp::xmtpv4::payer_api::PublishClientEnvelopesRequest;

    #[xmtp_common::test]
    fn test_group_message_response_extractor_with_empty_envelope() {
        let response = get_newest_envelope_response::Response {
            originator_envelope: None,
        };

        let mut extractor = MessageMetadataExtractor::new();

        // Test that the extractor handles empty responses gracefully
        let result = extractor.visit_newest_envelope_response(&response);
        assert!(
            result.is_ok(),
            "Extractor should handle empty response without error"
        );

        let responses = extractor.get();
        assert_eq!(responses.len(), 1, "Should create exactly one response");

        let extracted_response = &responses[0];
        assert!(
            extracted_response.is_none(),
            "Should have no group message for empty envelope"
        );
    }

    #[xmtp_common::test]
    fn test_group_message_response_extractor_builder_pattern() {
        // Test that the extractor can be built and used
        let extractor = MessageMetadataExtractor::new();
        let responses = extractor.get();
        assert_eq!(responses.len(), 0, "New extractor should have no responses");

        // Test default construction
        let extractor2: MessageMetadataExtractor = Default::default();
        let responses2 = extractor2.get();
        assert_eq!(
            responses2.len(),
            0,
            "Default extractor should have no responses"
        );
    }

    proptest! {
        #[xmtp_common::test]
        fn test_send_group_messages_with_dependencies(generated in group_message_request(15)) {
            let mut client = D14nClient::new_mock_with_store(TestCursorStore::default());
            client.cursor_store.dependencies = generated.dependencies;
            let request = generated.request.clone();
            client.client.expect_request().times(1).returning(move |_,_, mut body| {
                let body: PublishClientEnvelopesRequest = prost::Message::decode(&mut body).unwrap();
                for e in body.envelopes {
                    assert!(e.aad.is_some());
                    let aad = e.aad.unwrap();
                    assert!(aad.depends_on.is_some());
                }
                Ok(http::Response::new(request.encode_to_vec().into()))
            });

            client.send_group_messages(generated.request).now_or_never().unwrap().unwrap();
        }
    }
}
