use super::{SmartContractSignatureVerifier, ValidationResponse, VerifierError};
use crate::associations::AccountId;
use ethers::types::{BlockNumber, Bytes};

use xmtp_proto::{
    api_client::trait_impls::XmtpApi,
    xmtp::identity::api::v1::{
        VerifySmartContractWalletSignatureRequestSignature,
        VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
    },
};

pub struct RemoteSignatureVerifier<C> {
    identity_client: C,
}

impl<C> RemoteSignatureVerifier<C> {
    pub fn new(identity_client: C) -> Self {
        Self { identity_client }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<C> SmartContractSignatureVerifier for RemoteSignatureVerifier<C>
where
    C: XmtpApi,
{
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        let block_number = block_number.and_then(|bn| bn.as_number()).map(|bn| bn.0[0]);

        let result = self
            .identity_client
            .verify_smart_contract_wallet_signatures(VerifySmartContractWalletSignaturesRequest {
                signatures: vec![VerifySmartContractWalletSignatureRequestSignature {
                    account_id: account_id.into(),
                    block_number,
                    signature: signature.to_vec(),
                    hash: hash.to_vec(),
                }],
            })
            .await?;

        let VerifySmartContractWalletSignaturesResponse { responses } = result;

        Ok((&responses[0]).into())
    }
}

impl<T> Clone for RemoteSignatureVerifier<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            identity_client: self.identity_client.clone(),
        }
    }
}
