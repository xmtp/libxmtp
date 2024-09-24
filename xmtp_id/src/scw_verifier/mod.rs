mod chain_rpc_verifier;
mod url_parser;

use std::collections::HashMap;

use async_trait::async_trait;
use ethers::{
    providers::{Http, Provider},
    types::{BlockNumber, Bytes},
};
use thiserror::Error;

use crate::associations::AccountId;

pub use self::chain_rpc_verifier::*;

#[derive(Debug, Error)]
pub enum VerifierError {
    #[error("calling smart contract {0}")]
    Contract(#[from] ethers::contract::ContractError<Provider<Http>>),
    #[error("unexpected result from ERC-6492 {0}")]
    UnexpectedERC6492Result(String),
    #[error(transparent)]
    FromHex(#[from] hex::FromHexError),
    #[error(transparent)]
    Abi(#[from] ethers::abi::Error),
    #[error(transparent)]
    Provider(#[from] ethers::providers::ProviderError),
}

#[async_trait]
pub trait SmartContractSignatureVerifier: Send + Sync + 'static {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: &Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<bool, VerifierError>;
}

pub struct ChainSmartContractWalletVerifier {
    verifiers: HashMap<u64, Box<dyn SmartContractSignatureVerifier>>,
}

impl ChainSmartContractWalletVerifier {
    pub fn new(urls: HashMap<u64, url::Url>) -> Self {
        let verifiers: HashMap<u64, Box<dyn SmartContractSignatureVerifier>> = urls
            .into_iter()
            .map(|(chain_id, url)| {
                (
                    chain_id,
                    Box::new(RpcSmartContractWalletVerifier::new(url.to_string()))
                        as Box<dyn SmartContractSignatureVerifier>,
                )
            })
            .collect();

        Self { verifiers }
    }
}

#[async_trait]
impl SmartContractSignatureVerifier for ChainSmartContractWalletVerifier {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: &Bytes,
        _block_number: Option<BlockNumber>,
    ) -> Result<bool, VerifierError> {
        let id: u64 = account_id.chain_id.parse().unwrap();
        if let Some(verifier) = self.verifiers.get(&id) {
            return Ok(verifier
                .is_valid_signature(account_id, hash, signature, None)
                .await
                .unwrap());
        }

        todo!()
    }
}
