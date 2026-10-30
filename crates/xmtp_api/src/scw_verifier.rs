use crate::ApiClientWrapper;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::scw_verifier::VerifierError;
use xmtp_id::scw_verifier::{ScwInput, ValidationResponse};
use xmtp_id::{BlockNumber, Bytes, associations::AccountId};
use xmtp_proto::backend_v1::VerifySmartContractWalletSignaturesRequest;
use xmtp_proto::backend_v1::VerifySmartContractWalletSignaturesResponse;
use xmtp_proto::backend_v1::verify_smart_contract_wallet_signatures_request::Signature as VerifySmartContractWalletSignatureRequestSignature;
use xmtp_proto::backend_v1::verify_smart_contract_wallet_signatures_response::Response as VerifySmartContractWalletSignatureResponse;
use xmtp_proto::prelude::XmtpBackendClient;

fn verdict(response: VerifySmartContractWalletSignatureResponse) -> ValidationResponse {
    ValidationResponse {
        is_valid: response.is_valid,
        block_number: response.block_number,
        error: response.error,
    }
}

#[xmtp_common::async_trait]
impl<C> SmartContractSignatureVerifier for ApiClientWrapper<C>
where
    C: XmtpBackendClient,
{
    /// Verifies an ERC-6492<https://eips.ethereum.org/EIPS/eip-6492> signature.
    ///
    /// # Arguments
    ///
    /// * `signer` - can be the smart wallet address or EOA address.
    /// * `hash` - Message digest for the signature.
    /// * `signature` - Could be encoded smart wallet signature or raw ECDSA signature.
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        let result = self
            .verify_smart_contract_wallet_signatures(VerifySmartContractWalletSignaturesRequest {
                signatures: vec![VerifySmartContractWalletSignatureRequestSignature {
                    account_id: account_id.into(),
                    block_number,
                    signature: signature.to_vec(),
                    hash: hash.to_vec(),
                }],
            })
            .await
            .map_err(|e| VerifierError::Other(Box::new(e) as Box<_>))?;

        let VerifySmartContractWalletSignaturesResponse { responses } = result;

        let response =
            responses
                .into_iter()
                .next()
                .ok_or(VerifierError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "API returned empty response for signature verification request",
                )))?;
        Ok(verdict(response))
    }

    /// Verify the inputs in as few backend requests as the published limits
    /// allow, and return the verdicts in input order. The split and the
    /// verdict-count check are those of
    /// [`ApiClientWrapper::verify_smart_contract_wallet_signatures`]; its
    /// error keeps its retryability.
    async fn verify_many(
        &self,
        inputs: &[ScwInput],
    ) -> Result<Vec<ValidationResponse>, VerifierError> {
        let signatures = inputs
            .iter()
            .map(|input| VerifySmartContractWalletSignatureRequestSignature {
                account_id: input.account_id().clone().into(),
                block_number: input.block_number(),
                signature: input.signature().to_vec(),
                hash: input.hash().to_vec(),
            })
            .collect();
        let VerifySmartContractWalletSignaturesResponse { responses } = self
            .verify_smart_contract_wallet_signatures(VerifySmartContractWalletSignaturesRequest {
                signatures,
            })
            .await
            .map_err(|e| VerifierError::Other(Box::new(e) as Box<_>))?;
        Ok(responses.into_iter().map(verdict).collect())
    }
}
