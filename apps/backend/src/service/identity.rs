use super::query::count;
use crate::{Backend, api, validation::lookup_key};
use tonic::{Request, Response, Status};
use xmtp_common::RetryableError;
use xmtp_id::{associations::AccountId, scw_verifier::SmartContractSignatureVerifier};

#[tonic::async_trait]
impl api::identity_service_server::IdentityService for Backend {
    #[xmtp_common::rpc_span]
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
        let matches = self.store.lookup(&identifiers, &kinds).await?;
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
                Ok((account, hash, input.signature, input.block_number))
            })
            .collect::<Result<Vec<_>, Status>>()?;
        let mut responses = Vec::with_capacity(inputs.len());
        for (account, hash, signature, block) in inputs {
            let result = self
                .verifier
                .is_valid_signature(account, hash, signature.into(), block)
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
