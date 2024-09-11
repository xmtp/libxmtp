mod chain_rpc_verifier;

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
pub trait SmartContractSignatureVerifier: std::fmt::Debug + Send + Sync + 'static {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: &Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<bool, VerifierError>;
}
