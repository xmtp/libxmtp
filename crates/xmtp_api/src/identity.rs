use crate::{
    ApiClientWrapper, ApiError, HandleRejection, PublishUnit, Result,
    chunk::{MAX_READ_CHUNKS_IN_FLIGHT, publish_error_reason},
    dyn_err,
};
use futures::{StreamExt, TryStreamExt, stream};
use prost::Message;
use std::collections::{HashMap, HashSet};
use xmtp_configuration::MAX_SCW_SIGNATURES_PER_REQUEST;
use xmtp_proto::{
    api::grpc_status,
    api_client::XmtpBackendClient,
    backend_v1 as wire,
    types::{ApiIdentifier, Cursor, IdentityUpdateLog, IncomingBatchLimits, Topic, TopicCursor},
    xmtp::identity::associations::{IdentifierKind, IdentityUpdate},
};

/// The handle rejection that a refused identity publish reports in its
/// structured detail.
fn handle_rejection(error: &ApiError) -> Option<HandleRejection> {
    use wire::publish_error::Reason;
    match publish_error_reason(error)? {
        Reason::HandleTaken => Some(HandleRejection::Taken),
        Reason::HandleNotAllowed => Some(HandleRejection::NotAllowed),
        Reason::HandleTooSoon => Some(HandleRejection::TooSoon),
        _ => None,
    }
}

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
        let unit = PublishUnit::single_within(
            wire::ClientEnvelope {
                payload: Some(wire::client_envelope::Payload::IdentityUpdate(
                    update.into(),
                )),
            },
            self.limits(),
        )?;
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
            Err(error) => Err(handle_rejection(&error)
                .map(ApiError::HandleRejected)
                .unwrap_or(error)),
        }
    }
    /// Read every update the filters name and return them by inbox, in
    /// sequence order. Use [`Self::fold_identity_updates`] to store a long
    /// history without retaining it all.
    #[xmtp_common::rpc_span]
    pub async fn get_identity_updates_v2(
        &self,
        filters: Vec<GetIdentityUpdatesV2Filter>,
    ) -> Result<HashMap<String, Vec<IdentityUpdateLog>>> {
        let mut result: HashMap<_, Vec<_>> = filters
            .iter()
            .map(|filter| (filter.inbox_id.clone(), Vec::new()))
            .collect();
        self.fold_identity_updates(filters, IncomingBatchLimits::STREAM_BATCH, |updates| {
            for update in updates {
                result
                    .entry(update.update.inbox_id.clone())
                    .or_default()
                    .push(update);
            }
            Ok::<_, ApiError>(())
        })
        .await?;
        Ok(result)
    }

    /// Read the updates the filters name one bounded page at a time, in
    /// sequence order for each inbox, without retaining earlier pages. Every
    /// update names one of the filters' inboxes exactly. An API or fold error
    /// stops the read; the caller stores each page before the next request.
    #[xmtp_common::rpc_span]
    pub async fn fold_identity_updates<E: From<ApiError> + std::fmt::Display>(
        &self,
        filters: Vec<GetIdentityUpdatesV2Filter>,
        limits: IncomingBatchLimits,
        mut fold: impl FnMut(Vec<IdentityUpdateLog>) -> std::result::Result<(), E>,
    ) -> std::result::Result<(), E> {
        let mut cursors = TopicCursor::new();
        for filter in &filters {
            let bytes =
                hex::decode(&filter.inbox_id).map_err(|_| ApiError::InvalidRequest("inbox id"))?;
            let topic = Topic::new_identity_update(bytes);
            Topic::parse(&topic).map_err(ApiError::from)?;
            let after = Cursor(filter.sequence_id.unwrap_or(0));
            cursors
                .entry(topic)
                .and_modify(|cursor| *cursor = (*cursor).min(after))
                .or_insert(after);
        }
        let requested: HashSet<_> = filters.iter().map(|filter| &filter.inbox_id).collect();
        self.fold_ordered_pages(cursors, limits, |batches, _| {
            let updates = batches
                .into_iter()
                .flat_map(|batch| batch.envelopes)
                .map(|envelope| {
                    let update = xmtp_api_backend::envelope::decode_identity_update(envelope)?;
                    if !requested.contains(&update.update.inbox_id) {
                        return Err(ApiError::InvalidResponse("unrequested inbox"));
                    }
                    Ok(update)
                })
                .collect::<Result<Vec<_>>>()?;
            fold(updates)
        })
        .await
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
            .chunks(self.limits().max_lookup_identifiers)
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
    /// Verify every signature and return one verdict per signature, in
    /// request order. The signatures go out in requests of at most the
    /// published `max_scw_signatures`, never more than
    /// [`MAX_SCW_SIGNATURES_PER_REQUEST`], and within `max_request_bytes`,
    /// with up to [`MAX_READ_CHUNKS_IN_FLIGHT`] requests in flight. A
    /// response whose verdict count differs from its request's is a
    /// retryable [`ApiError::VerdictCount`].
    #[xmtp_common::rpc_span]
    pub async fn verify_smart_contract_wallet_signatures(
        &self,
        request: wire::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<wire::VerifySmartContractWalletSignaturesResponse> {
        let limits = self.limits();
        let per_request = limits
            .max_scw_signatures
            .min(MAX_SCW_SIGNATURES_PER_REQUEST);
        let requests: Vec<_> =
            verify_requests(&request.signatures, per_request, limits.max_request_bytes)
                .into_iter()
                .map(<[_]>::to_vec)
                .collect();
        let chunks: Vec<Vec<_>> = stream::iter(
            requests
                .into_iter()
                .map(|signatures| self.verify_chunk(signatures)),
        )
        .buffered(MAX_READ_CHUNKS_IN_FLIGHT)
        .try_collect()
        .await?;
        Ok(wire::VerifySmartContractWalletSignaturesResponse {
            responses: chunks.into_iter().flatten().collect(),
        })
    }

    /// Verify the signatures of one request. The snapshot it was sized by
    /// can be above the deployment's current limits, so a refusal of the
    /// whole request as too large sends its halves instead, one after the
    /// other, down to single signatures. A single signature's refusal is
    /// returned.
    async fn verify_chunk(
        &self,
        signatures: Vec<wire::verify_smart_contract_wallet_signatures_request::Signature>,
    ) -> Result<Vec<wire::verify_smart_contract_wallet_signatures_response::Response>> {
        let mut verdicts = Vec::with_capacity(signatures.len());
        // The next request is the last one pushed, so halves go out in order.
        let mut pending = vec![signatures];
        while let Some(signatures) = pending.pop() {
            let request = wire::VerifySmartContractWalletSignaturesRequest { signatures };
            let response = match self
                .retry_call(
                    || {
                        self.api_client
                            .verify_smart_contract_wallet_signatures(request.clone())
                    },
                    false,
                )
                .await
            {
                Ok(response) => response,
                Err(error) if request.signatures.len() > 1 && refused_as_too_large(&error) => {
                    let mut first = request.signatures;
                    let second = first.split_off(first.len() / 2);
                    pending.extend([second, first]);
                    continue;
                }
                Err(error) => return Err(dyn_err(error)),
            };
            if response.responses.len() != request.signatures.len() {
                return Err(ApiError::VerdictCount {
                    expected: request.signatures.len(),
                    got: response.responses.len(),
                });
            }
            verdicts.extend(response.responses);
        }
        Ok(verdicts)
    }
}

/// A refusal of a whole verify request that a smaller request can pass: the
/// count limit (`INVALID_ARGUMENT`) or the byte limit (`OUT_OF_RANGE`). The
/// backend also answers `INVALID_ARGUMENT` for one signature it refuses, so
/// splitting ends at that signature with its own refusal. A rate-limit
/// rejection is never split, whatever its status code.
// implements: RATE-041
fn refused_as_too_large(error: &(dyn std::error::Error + 'static)) -> bool {
    crate::preflight::failure(error).is_none()
        && xmtp_api_grpc::error::rate_limit_delay(error).is_none()
        && grpc_status(error).is_some_and(|status| {
            matches!(
                status.code(),
                tonic::Code::InvalidArgument | tonic::Code::OutOfRange
            )
        })
}

/// Split `signatures` into verify requests the deployment accepts. A request
/// ends at `per_request` signatures, or before the signature whose framed
/// length would take its encoding past `max_request_bytes`. Every request
/// holds at least one signature, so one that alone exceeds the byte bound is
/// sent by itself.
// implements: CONF-073
fn verify_requests(
    signatures: &[wire::verify_smart_contract_wallet_signatures_request::Signature],
    per_request: usize,
    max_request_bytes: usize,
) -> Vec<&[wire::verify_smart_contract_wallet_signatures_request::Signature]> {
    let mut requests = Vec::new();
    let (mut start, mut bytes) = (0, 0);
    for (end, signature) in signatures.iter().enumerate() {
        let len = signature.encoded_len();
        let framed = 1 + prost::length_delimiter_len(len) + len;
        if end > start && (end - start >= per_request || bytes + framed > max_request_bytes) {
            requests.push(&signatures[start..end]);
            (start, bytes) = (end, 0);
        }
        bytes += framed;
    }
    if start < signatures.len() {
        requests.push(&signatures[start..]);
    }
    requests
}
