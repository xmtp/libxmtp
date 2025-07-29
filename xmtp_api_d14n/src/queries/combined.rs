use crate::d14n::GetNewestEnvelopes;
use crate::d14n::QueryEnvelope;
use crate::protocol::CollectionExtractor;
use crate::protocol::GroupMessageExtractor;
use crate::protocol::IdentityUpdateExtractor;
use crate::protocol::KeyPackagesExtractor;
use crate::protocol::SequencedExtractor;
use crate::protocol::TopicKind;
use crate::protocol::WelcomeMessageExtractor;
use crate::protocol::traits::EnvelopeCollection;
use crate::protocol::traits::Extractor;
use crate::v3::PublishCommitLog;
use crate::v3::PublishIdentityUpdate;
use crate::v3::QueryCommitLog;
use crate::v3::VerifySmartContractWalletSignatures;
use crate::v3::{SendGroupMessages, SendWelcomeMessages, UploadKeyPackage};
use crate::{d14n::QueryEnvelopes, endpoints::d14n::GetInboxIds as GetInboxIdsV4};
use itertools::Itertools;
use std::collections::HashMap;
use xmtp_common::RetryableError;
use xmtp_cursor_state::store::SharedCursorStore;
use xmtp_proto::api_client::ApiStats;
use xmtp_proto::api_client::IdentityStats;
use xmtp_proto::api_client::XmtpMlsClient;
use xmtp_proto::identity_v1;
use xmtp_proto::identity_v1::get_identity_updates_response::IdentityUpdateLog;
use xmtp_proto::mls_v1;
use xmtp_proto::prelude::XmtpIdentityClient;
use xmtp_proto::traits::Client;
use xmtp_proto::traits::{ApiClientError, Query};
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_response::Response;
use xmtp_proto::xmtp::identity::associations::IdentifierKind;
use xmtp_proto::xmtp::xmtpv4::envelopes::Cursor;
use xmtp_proto::xmtp::xmtpv4::message_api::GetNewestEnvelopeResponse;
use xmtp_proto::xmtp::xmtpv4::message_api::QueryEnvelopesResponse;
use xmtp_proto::xmtp::xmtpv4::message_api::{
    EnvelopesQuery, GetInboxIdsResponse as GetInboxIdsResponseV4,
};

#[derive(Clone)]
pub struct CombinedD14nClient<C, D> {
    pub(crate) v3_client: C,
    pub(crate) xmtpd_client: D,
    pub(crate) cursor_store: SharedCursorStore,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, D, E> XmtpMlsClient for CombinedD14nClient<C, D>
where
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    C: Send + Sync + Client<Error = E>,
    D: Send + Sync + Client<Error = E>,
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
            .query(&self.v3_client)
            .await
    }

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
            .query(&self.xmtpd_client)
            .await?;
        let extractor = CollectionExtractor::new(result.results, KeyPackagesExtractor::new());
        let key_packages = extractor.get()?;
        Ok(mls_v1::FetchKeyPackagesResponse { key_packages })
    }

    async fn send_group_messages(
        &self,
        request: mls_v1::SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        SendGroupMessages::builder()
            .messages(request.messages)
            .build()?
            .query(&self.v3_client)
            .await
    }

    async fn send_welcome_messages(
        &self,
        request: mls_v1::SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        SendWelcomeMessages::builder()
            .messages(request.messages)
            .build()?
            .query(&self.v3_client)
            .await
    }
    async fn query_group_messages(
        &self,
        request: mls_v1::QueryGroupMessagesRequest,
    ) -> Result<mls_v1::QueryGroupMessagesResponse, Self::Error> {
        let topic = TopicKind::GroupMessagesV1.build(request.group_id.as_slice());
        let response: QueryEnvelopesResponse = QueryEnvelope::builder(self.cursor_store.clone())
            .topic(topic.clone())
            .paging_info(request.paging_info)
            .build()?
            .query(&self.xmtpd_client)
            .await?;

        let extracted = SequencedExtractor::builder()
            .envelopes(response.envelopes)
            .build::<GroupMessageExtractor>()
            .get()?;

        let mut store = self.cursor_store.lock().unwrap();
        for (_msg, node_id, seq_id) in &extracted {
            store.processed(
                topic.clone(),
                &Cursor {
                    node_id_to_sequence_id: [(*node_id, *seq_id)].into(),
                },
            );
        }

        let messages = extracted
            .into_iter()
            .map(|(msg, _, _)| msg)
            .collect::<Vec<_>>();

        Ok(mls_v1::QueryGroupMessagesResponse {
            messages,
            paging_info: None,
        })
    }
    async fn query_welcome_messages(
        &self,
        request: mls_v1::QueryWelcomeMessagesRequest,
    ) -> Result<mls_v1::QueryWelcomeMessagesResponse, Self::Error> {
        let topic = TopicKind::WelcomeMessagesV1.build(request.installation_key.as_slice());

        let response = QueryEnvelope::builder(self.cursor_store.clone())
            .topic(topic.clone())
            .paging_info(request.paging_info)
            .build()?
            .query(&self.xmtpd_client)
            .await?;

        let extracted = SequencedExtractor::builder()
            .envelopes(response.envelopes)
            .build::<WelcomeMessageExtractor>()
            .get()?;

        let mut store = self.cursor_store.lock().unwrap();
        for (_msg, node_id, seq_id) in &extracted {
            store.processed(
                topic.clone(),
                &Cursor {
                    node_id_to_sequence_id: [(*node_id, *seq_id)].into(),
                },
            );
        }

        let messages = extracted
            .into_iter()
            .map(|(msg, _, _)| msg)
            .collect::<Vec<_>>();

        Ok(mls_v1::QueryWelcomeMessagesResponse {
            messages,
            paging_info: None,
        })
    }

    async fn publish_commit_log(
        &self,
        request: mls_v1::BatchPublishCommitLogRequest,
    ) -> Result<(), Self::Error> {
        PublishCommitLog::builder()
            .commit_log_entries(request.requests)
            .build()?
            .query(&self.xmtpd_client)
            .await
    }

    async fn query_commit_log(
        &self,
        request: mls_v1::BatchQueryCommitLogRequest,
    ) -> Result<mls_v1::BatchQueryCommitLogResponse, Self::Error> {
        QueryCommitLog::builder()
            .query_log_requests(request.requests)
            .build()?
            .query(&self.xmtpd_client)
            .await
    }

    fn stats(&self) -> ApiStats {
        Default::default()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, D, E> XmtpIdentityClient for CombinedD14nClient<C, D>
where
    C: Send + Sync + Client<Error = E>,
    D: Send + Sync + Client<Error = E>,
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    ApiClientError<E>: From<ApiClientError<<D as xmtp_proto::traits::Client>::Error>>,
{
    type Error = ApiClientError<E>;

    async fn publish_identity_update(
        &self,
        request: identity_v1::PublishIdentityUpdateRequest,
    ) -> Result<identity_v1::PublishIdentityUpdateResponse, Self::Error> {
        PublishIdentityUpdate::builder()
            //todo: handle error or tryFrom
            .identity_update(request.identity_update)
            .build()?
            .query(&self.v3_client)
            .await
    }

    async fn get_identity_updates_v2(
        &self,
        request: identity_v1::GetIdentityUpdatesRequest,
    ) -> Result<identity_v1::GetIdentityUpdatesResponse, Self::Error> {
        if request.requests.is_empty() {
            return Ok(identity_v1::GetIdentityUpdatesResponse { responses: vec![] });
        }

        let topics = request.requests.topics()?;
        //todo: replace with returned node_id
        let node_id = 100;
        let last_seen = Some(Cursor {
            node_id_to_sequence_id: [(node_id, request.requests.first().unwrap().sequence_id)]
                .into(),
        });
        let result: QueryEnvelopesResponse = QueryEnvelopes::builder()
            .envelopes(EnvelopesQuery {
                topics: topics.clone(),
                originator_node_ids: vec![],
                last_seen,
            })
            .build()?
            .query(&self.xmtpd_client)
            .await?;

        let updates: HashMap<String, Vec<IdentityUpdateLog>> = SequencedExtractor::builder()
            .envelopes(result.envelopes)
            .build::<IdentityUpdateExtractor>()
            .get()?
            .into_iter()
            .into_group_map();

        let responses = updates
            .into_iter()
            .map(|(inbox_id, updates)| Response { updates, inbox_id })
            .collect();
        Ok(identity_v1::GetIdentityUpdatesResponse { responses })
    }

    async fn get_inbox_ids(
        &self,
        request: identity_v1::GetInboxIdsRequest,
    ) -> Result<identity_v1::GetInboxIdsResponse, Self::Error> {
        let res: GetInboxIdsResponseV4 = GetInboxIdsV4::builder()
            .addresses(
                request
                    .requests
                    .iter()
                    .filter(|r| r.identifier_kind == IdentifierKind::Ethereum as i32)
                    .map(|r| r.identifier.clone())
                    .collect::<Vec<_>>(),
            )
            .passkeys(
                request
                    .requests
                    .iter()
                    .filter(|r| r.identifier_kind == IdentifierKind::Passkey as i32)
                    .map(|r| r.identifier.clone())
                    .collect::<Vec<_>>(),
            )
            .build()?
            .query(&self.xmtpd_client)
            .await?;

        Ok(identity_v1::GetInboxIdsResponse {
            responses: res
                .responses
                .iter()
                .map(|r| identity_v1::get_inbox_ids_response::Response {
                    identifier: r.identifier.clone(),
                    identifier_kind: IdentifierKind::Ethereum as i32,
                    inbox_id: r.inbox_id.clone(),
                })
                .collect::<Vec<_>>(),
        })
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: identity_v1::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<identity_v1::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        VerifySmartContractWalletSignatures::builder()
            .signatures(request.signatures)
            .build()?
            .query(&self.xmtpd_client)
            .await
    }

    fn identity_stats(&self) -> IdentityStats {
        Default::default()
    }
}
