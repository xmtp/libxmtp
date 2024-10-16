use super::{SmartContractSignatureVerifier, ValidationResponse, VerifierError};
use crate::associations::AccountId;
use async_trait::async_trait;
use ethers::types::{BlockNumber, Bytes};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Channel;

use xmtp_proto::xmtp::identity::api::v1::{
    identity_api_client::IdentityApiClient, VerifySmartContractWalletSignatureRequestSignature,
    VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
};

#[derive(Clone)]
pub struct RemoteSignatureVerifier {
    identity_client: Arc<Mutex<IdentityApiClient<Channel>>>,
}

impl RemoteSignatureVerifier {
    pub fn new(identity_client: IdentityApiClient<Channel>) -> Self {
        Self {
            identity_client: Arc::new(Mutex::new(identity_client)),
        }
    }
}

#[async_trait]
impl SmartContractSignatureVerifier for RemoteSignatureVerifier {
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
            .lock()
            .await
            .verify_smart_contract_wallet_signatures(VerifySmartContractWalletSignaturesRequest {
                signatures: vec![VerifySmartContractWalletSignatureRequestSignature {
                    account_id: account_id.into(),
                    block_number,
                    signature: signature.to_vec(),
                    hash: hash.to_vec(),
                }],
            })
            .await
            .map_err(VerifierError::Tonic)?;

        let VerifySmartContractWalletSignaturesResponse { responses } = result.into_inner();

        Ok(responses
            .into_iter()
            .next()
            .expect("Api given one request will return one response")
            .into())
    }
}
