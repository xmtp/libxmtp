use crate::{
    ApiClientWrapper, ApiError, PublishUnit, Result, chunk::MAX_READ_CHUNKS_IN_FLIGHT, dyn_err,
};
use futures::{StreamExt, TryStreamExt, stream};
use std::collections::HashMap;
use xmtp_configuration::{
    BACKEND_DEFAULT_MAX_LOOKUP_IDENTIFIERS, BACKEND_DEFAULT_MAX_QUERY_LIMIT,
    BACKEND_DEFAULT_MAX_SCW_SIGNATURES,
};
use xmtp_proto::{
    api::grpc_status,
    api_client::XmtpBackendClient,
    backend_v1 as wire,
    types::{ApiIdentifier, Cursor, IdentityUpdateLog, Topic, TopicCursor},
    xmtp::identity::associations::{IdentifierKind, IdentityUpdate},
};

/// Read updates after the exclusive sequence id on this inbox topic.
#[derive(Debug)]
pub struct GetIdentityUpdatesV2Filter {
    pub inbox_id: String,
    pub sequence_id: Option<u64>,
}

impl<C: XmtpBackendClient> ApiClientWrapper<C> {
    #[xmtp_common::rpc_span]
    pub async fn publish_identity_update<U: Into<IdentityUpdate>>(
        &self,
        update: U,
    ) -> Result<Cursor> {
        let unit = PublishUnit::single(wire::ClientEnvelope {
            payload: Some(wire::client_envelope::Payload::IdentityUpdate(
                update.into(),
            )),
        })?;
        match self.publish_units(vec![unit]).await {
            Ok(metas) => metas
                .into_iter()
                .next()
                .and_then(|meta| meta.cursor)
                .map(Into::into)
                .ok_or(ApiError::InvalidResponse("identity publish cursor")),
            Err(error)
                if grpc_status(&error)
                    .is_some_and(|status| status.code() == tonic::Code::Aborted) =>
            {
                Err(ApiError::IdentityUpdateConflict)
            }
            Err(error) => Err(error),
        }
    }
    #[xmtp_common::rpc_span]
    pub async fn get_identity_updates_v2(
        &self,
        filters: Vec<GetIdentityUpdatesV2Filter>,
    ) -> Result<HashMap<String, Vec<IdentityUpdateLog>>> {
        let mut cursors = TopicCursor::new();
        let mut result = HashMap::new();
        for filter in filters {
            let bytes =
                hex::decode(&filter.inbox_id).map_err(|_| ApiError::InvalidRequest("inbox id"))?;
            let topic = Topic::new_identity_update(bytes);
            Topic::parse(&topic)?;
            cursors
                .entry(topic)
                .and_modify(|cursor| {
                    *cursor = (*cursor).min(Cursor(filter.sequence_id.unwrap_or(0)))
                })
                .or_insert(Cursor(filter.sequence_id.unwrap_or(0)));
            result.entry(filter.inbox_id).or_insert_with(Vec::new);
        }
        for envelope in self
            .query_all(cursors, BACKEND_DEFAULT_MAX_QUERY_LIMIT as u32)
            .await?
        {
            let update = xmtp_api_backend::envelope::decode_identity_update(envelope)?;
            result
                .get_mut(&update.update.inbox_id)
                .ok_or(ApiError::InvalidResponse("unrequested inbox"))?
                .push(update);
        }
        Ok(result)
    }
    /// Return one optional inbox id for every input, in the same order.
    #[xmtp_common::rpc_span]
    pub async fn get_inbox_ids(
        &self,
        identifiers: Vec<ApiIdentifier>,
    ) -> Result<Vec<Option<String>>> {
        if identifiers
            .iter()
            .any(|id| id.identifier_kind == IdentifierKind::Unspecified)
        {
            return Err(ApiError::InvalidResponse("unspecified identifier kind"));
        }
        let requests: Vec<_> = identifiers
            .chunks(BACKEND_DEFAULT_MAX_LOOKUP_IDENTIFIERS)
            .map(<[_]>::to_vec)
            .collect();
        let mut chunks: Vec<_> = stream::iter(requests.into_iter().enumerate().map(
            |(index, chunk)| async move {
                let request = wire::GetInboxIdsRequest {
                    requests: chunk
                        .iter()
                        .map(|id| wire::get_inbox_ids_request::Request {
                            identifier: id.identifier.clone(),
                            identifier_kind: id.identifier_kind as i32,
                        })
                        .collect(),
                };
                let response = self
                    .retry_call(|| self.api_client.get_inbox_ids(request.clone()), false)
                    .await
                    .map_err(dyn_err)?;
                if response.responses.len() != chunk.len() {
                    return Err(ApiError::InvalidResponse("inbox result count"));
                }
                let mut values = Vec::with_capacity(chunk.len());
                for (response, input) in response.responses.into_iter().zip(&chunk) {
                    if IdentifierKind::try_from(response.identifier_kind)
                        .ok()
                        .filter(|kind| *kind != IdentifierKind::Unspecified)
                        != Some(input.identifier_kind)
                        || response.identifier != input.identifier
                    {
                        return Err(ApiError::InvalidResponse("inbox result identity"));
                    }
                    values.push(response.inbox_id);
                }
                Ok((index, values))
            },
        ))
        .buffer_unordered(MAX_READ_CHUNKS_IN_FLIGHT)
        .try_collect()
        .await?;
        chunks.sort_by_key(|(index, _)| *index);
        Ok(chunks.into_iter().flat_map(|(_, values)| values).collect())
    }
    #[xmtp_common::rpc_span]
    pub async fn verify_smart_contract_wallet_signatures(
        &self,
        request: wire::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<wire::VerifySmartContractWalletSignaturesResponse> {
        let requests: Vec<_> = request
            .signatures
            .chunks(BACKEND_DEFAULT_MAX_SCW_SIGNATURES)
            .map(<[_]>::to_vec)
            .collect();
        let mut chunks: Vec<_> = stream::iter(requests.into_iter().enumerate().map(
            |(index, chunk)| async move {
                let request = wire::VerifySmartContractWalletSignaturesRequest {
                    signatures: chunk.to_vec(),
                };
                let response = self
                    .retry_call(
                        || {
                            self.api_client
                                .verify_smart_contract_wallet_signatures(request.clone())
                        },
                        false,
                    )
                    .await
                    .map_err(dyn_err)?;
                if response.responses.len() != chunk.len() {
                    return Err(ApiError::InvalidResponse("signature result count"));
                }
                Ok((index, response.responses))
            },
        ))
        .buffer_unordered(MAX_READ_CHUNKS_IN_FLIGHT)
        .try_collect()
        .await?;
        chunks.sort_by_key(|(index, _)| *index);
        Ok(wire::VerifySmartContractWalletSignaturesResponse {
            responses: chunks.into_iter().flat_map(|(_, values)| values).collect(),
        })
    }
}
