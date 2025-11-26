use super::{SmartContractSignatureVerifier, ValidationResponse, VerifierError};
use crate::associations::AccountId;
use alloy::primitives::{BlockNumber, Bytes};

use xmtp_proto::{
    prelude::XmtpIdentityClient,
    xmtp::identity::api::v1::{
        VerifySmartContractWalletSignatureRequestSignature,
        VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
    },
};

pub struct RemoteSignatureVerifier<C> {
    api: C,
}

impl<ApiClient> RemoteSignatureVerifier<ApiClient> {
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }
}

#[xmtp_common::async_trait]
impl<C> SmartContractSignatureVerifier for RemoteSignatureVerifier<C>
where
    C: XmtpIdentityClient,
{
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        let result = self
            .api
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

impl<T> Clone for RemoteSignatureVerifier<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            api: self.api.clone(),
        }
    }
}
