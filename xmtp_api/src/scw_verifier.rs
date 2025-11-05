use crate::ApiClientWrapper;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::scw_verifier::VerifierError;
use xmtp_id::{BlockNumber, Bytes, associations::AccountId, scw_verifier::ValidationResponse};
use xmtp_proto::prelude::XmtpIdentityClient;
use xmtp_proto::xmtp::identity::api::v1::VerifySmartContractWalletSignatureRequestSignature;
use xmtp_proto::xmtp::identity::api::v1::VerifySmartContractWalletSignaturesRequest;
use xmtp_proto::xmtp::identity::api::v1::VerifySmartContractWalletSignaturesResponse;

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<C> SmartContractSignatureVerifier for ApiClientWrapper<C>
where
    C: XmtpIdentityClient,
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

        Ok(responses
            .into_iter()
            .next()
            .expect("Api given one request will return one response")
            .into())
    }
}
