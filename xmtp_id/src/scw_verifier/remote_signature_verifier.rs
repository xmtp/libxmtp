use super::{SmartContractSignatureVerifier, VerifierError};
use crate::associations::AccountId;
use async_trait::async_trait;
use ethers::providers::{Http, Middleware, Provider};
use ethers::types::{BlockNumber, Bytes, U64};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Channel;

use xmtp_proto::xmtp::identity::{
    api::v1::{
        identity_api_client::IdentityApiClient, UnverifiedSmartContractWalletSignature,
        VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
    },
    associations::SmartContractWalletSignature,
};

#[derive(Clone)]
struct RemoteSignatureVerifier {
    identity_client: Arc<Mutex<IdentityApiClient<Channel>>>,
    provider: Arc<Provider<Http>>,
}

#[async_trait]
impl SmartContractSignatureVerifier for RemoteSignatureVerifier {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<bool, VerifierError> {
        let block_number = match block_number {
            Some(BlockNumber::Number(block_number)) => block_number,
            _ => self.current_block_number(&account_id.chain_id).await?,
        };

        let result = self
            .identity_client
            .lock()
            .await
            .verify_smart_contract_wallet_signatures(VerifySmartContractWalletSignaturesRequest {
                signatures: vec![UnverifiedSmartContractWalletSignature {
                    scw_signature: Some(SmartContractWalletSignature {
                        account_id: account_id.into(),
                        block_number: block_number.0[0],
                        signature: signature.to_vec(),
                    }),
                    hash: hash.to_vec(),
                }],
            })
            .await
            .unwrap();

        let VerifySmartContractWalletSignaturesResponse { responses } = result.into_inner();

        Ok(responses[0].is_valid)
    }
    async fn current_block_number(&self, _chain_id: &str) -> Result<U64, VerifierError> {
        self.provider
            .get_block_number()
            .await
            .map_err(VerifierError::Provider)
    }
}
