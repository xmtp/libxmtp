use std::collections::HashMap;

use super::D14nClient;
use crate::protocol::IdentityUpdateExtractor;
use crate::protocol::SequencedExtractor;
use crate::protocol::traits::{Envelope, EnvelopeCollection, Extractor};
use crate::{d14n::PublishClientEnvelopes, d14n::QueryEnvelopes, endpoints::d14n::GetInboxIds};
use itertools::Itertools;
use xmtp_common::RetryableError;
use xmtp_proto::ConversionError;
use xmtp_proto::api_client::{IdentityStats, XmtpIdentityClient};
use xmtp_proto::identity_v1;
use xmtp_proto::identity_v1::get_identity_updates_response::IdentityUpdateLog;
use xmtp_proto::client_traits::Client;
use xmtp_proto::client_traits::{ApiClientError, Query};
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_response::Response;
use xmtp_proto::xmtp::identity::associations::IdentifierKind;
use xmtp_proto::xmtp::xmtpv4::envelopes::Cursor;
use xmtp_proto::xmtp::xmtpv4::message_api::{
    EnvelopesQuery, GetInboxIdsResponse as GetInboxIdsResponseV4, QueryEnvelopesResponse,
};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, P, E> XmtpIdentityClient for D14nClient<C, P>
where
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    P: Send + Sync + Client<Error = E>,
    C: Send + Sync + Client<Error = E>,
    ApiClientError<E>: From<ApiClientError<<P as xmtp_proto::client_traits::Client>::Error>>,
{
    type Error = ApiClientError<E>;

    #[tracing::instrument(level = "trace", skip_all)]
    async fn publish_identity_update(
        &self,
        request: identity_v1::PublishIdentityUpdateRequest,
    ) -> Result<identity_v1::PublishIdentityUpdateResponse, Self::Error> {
        let update = request.identity_update.ok_or(ConversionError::Missing {
            item: "identity_update",
            r#type: std::any::type_name::<identity_v1::PublishIdentityUpdateRequest>(),
        })?;

        let envelopes = update.client_envelope()?;
        PublishClientEnvelopes::builder()
            .envelope(envelopes)
            .build()?
            .query(&self.payer_client)
            .await?;

        Ok(identity_v1::PublishIdentityUpdateResponse {})
    }

    #[tracing::instrument(level = "trace", skip_all)]
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
            .query(&self.message_client)
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

    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_inbox_ids(
        &self,
        request: identity_v1::GetInboxIdsRequest,
    ) -> Result<identity_v1::GetInboxIdsResponse, Self::Error> {
        let res: GetInboxIdsResponseV4 = GetInboxIds::builder()
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
            .query(&self.message_client)
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

    #[tracing::instrument(level = "trace", skip_all)]
    async fn verify_smart_contract_wallet_signatures(
        &self,
        _request: identity_v1::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<identity_v1::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        unimplemented!()
    }

    fn identity_stats(&self) -> IdentityStats {
        Default::default()
    }
}
