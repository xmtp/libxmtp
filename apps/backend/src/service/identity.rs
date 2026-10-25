use super::query::count;
use crate::{Backend, api, validation::lookup_key};
use tonic::{Request, Response, Status};
use xmtp_common::RetryableError;
use xmtp_id::{
    associations::AccountId,
    scw_verifier::{ScwInput, SmartContractSignatureVerifier},
};

#[cfg(test)]
mod tests;

#[tonic::async_trait]
impl api::identity_service_server::IdentityService for Backend {
    #[xmtp_common::rpc_span]
    /// Resolve identifier requests in their original order.
    ///
    /// Input identifiers are normalized before the read. Missing associations
    /// stay absent in the corresponding response entry; the read pool may lag
    /// the primary when a replica is configured.
    // implements: API-270
    async fn get_inbox_ids(
        &self,
        request: Request<api::GetInboxIdsRequest>,
    ) -> Result<Response<api::GetInboxIdsResponse>, Status> {
        let request = request.into_inner();
        count(
            request.requests.len(),
            self.config.limits.max_lookup_identifiers,
        )?;
        let (identifiers, kinds): (Vec<_>, Vec<_>) = request
            .requests
            .iter()
            .map(|value| lookup_key(&value.identifier, value.identifier_kind))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .unzip();
        let matches = self.store.inbox_ids(&identifiers, &kinds).await?;
        let responses = request
            .requests
            .into_iter()
            .zip(matches)
            .map(|(value, inbox_id)| api::get_inbox_ids_response::Response {
                identifier: value.identifier,
                identifier_kind: value.identifier_kind,
                inbox_id: inbox_id.map(hex::encode),
            })
            .collect();
        Ok(Response::new(api::GetInboxIdsResponse { responses }))
    }

    #[xmtp_common::rpc_span]
    /// Verify each supplied smart-contract-wallet signature independently.
    ///
    /// Every entry is checked before the first verifier call, so one whose
    /// account id is not a non-zero `eip155` account, whose hash is not 32
    /// bytes, or whose signature is longer than `MAX_SCW_SIGNATURE_BYTES`
    /// fails the request with `INVALID_ARGUMENT` and reaches no chain or
    /// cache. Provider failures map to `UNAVAILABLE`, while a negative
    /// signature verdict is a successful response with `is_valid` set to false.
    // implements: API-231, API-301
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: Request<api::VerifySmartContractWalletSignaturesRequest>,
    ) -> Result<Response<api::VerifySmartContractWalletSignaturesResponse>, Status> {
        let request = request.into_inner();
        count(
            request.signatures.len(),
            self.config.limits.max_scw_signatures,
        )?;
        let inputs = request
            .signatures
            .into_iter()
            .map(|input| {
                let account = AccountId::try_from(input.account_id)
                    .map_err(|_| Status::invalid_argument("malformed account id"))?;
                let hash: [u8; 32] = input
                    .hash
                    .try_into()
                    .map_err(|_| Status::invalid_argument("hash must contain 32 bytes"))?;
                ScwInput::checked(account, hash, input.signature.into(), input.block_number)
                    .map_err(|_| Status::invalid_argument("unverifiable account id or signature"))
            })
            .collect::<Result<Vec<_>, Status>>()?;
        let mut responses = Vec::with_capacity(inputs.len());
        for input in inputs {
            let result = crate::validation::ObservedVerifier(&self.verifier)
                .is_valid_signature(
                    input.account_id().clone(),
                    input.hash(),
                    input.signature().clone(),
                    input.block_number(),
                )
                .await
                .map_err(|error| {
                    if error.is_retryable() {
                        Status::unavailable("signature verifier unavailable")
                    } else {
                        Status::invalid_argument("signature verification failed")
                    }
                })?;
            responses.push(
                api::verify_smart_contract_wallet_signatures_response::Response {
                    is_valid: result.is_valid,
                    block_number: result.block_number,
                    error: result.error,
                },
            );
        }
        Ok(Response::new(
            api::VerifySmartContractWalletSignaturesResponse { responses },
        ))
    }
}
