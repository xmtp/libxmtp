mod chain_rpc_verifier;
mod remote_signature_verifier;

use crate::associations::AccountId;
use async_trait::async_trait;
use dyn_clone::DynClone;
use ethers::{
    providers::{Http, Provider, ProviderError},
    types::{BlockNumber, Bytes},
};
use std::{
    collections::HashMap,
    env,
    fs::{self},
    io,
    path::Path,
};
use thiserror::Error;
use tracing::info;
use url::Url;

pub use chain_rpc_verifier::*;
pub use remote_signature_verifier::*;

static DEFAULT_CHAIN_URLS: &str = include_str!("chain_urls_default.json");

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
    #[error(transparent)]
    Tonic(tonic::Status),
}

#[async_trait]
pub trait SmartContractSignatureVerifier: Send + Sync + DynClone + 'static {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError>;
}

pub struct ValidationResponse {
    pub is_valid: bool,
    pub block_number: Option<u64>,
}

dyn_clone::clone_trait_object!(SmartContractSignatureVerifier);

#[async_trait]
impl<S: SmartContractSignatureVerifier + Clone> SmartContractSignatureVerifier for Box<S> {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        (**self)
            .is_valid_signature(account_id, hash, signature, block_number)
            .await
    }
}

#[derive(Clone)]
pub struct MultiSmartContractSignatureVerifier {
    verifiers: HashMap<String, Box<dyn SmartContractSignatureVerifier>>,
}

impl Default for MultiSmartContractSignatureVerifier {
    fn default() -> Self {
        let urls: HashMap<String, Url> =
            serde_json::from_str(DEFAULT_CHAIN_URLS).expect("DEFAULT_CHAIN_URLS is malformatted");
        Self::new(urls).upgrade()
    }
}

impl MultiSmartContractSignatureVerifier {
    pub fn new(urls: HashMap<String, url::Url>) -> Self {
        let verifiers = urls
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

    pub fn new_from_file(path: impl AsRef<Path>) -> Result<Self, io::Error> {
        let json = fs::read_to_string(path.as_ref())?;
        let urls: HashMap<String, Url> = serde_json::from_str(&json).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unable to deserialize json: {err:?}"),
            )
        })?;

        Ok(Self::new(urls))
    }

    /// Upgrade the default urls to paid/private/alternative urls if the env vars are present.
    pub fn upgrade(mut self) -> Self {
        self.verifiers.iter_mut().for_each(|(id, verif)| {
            if let Ok(url) = env::var(format!("CHAIN_RPC_{id}")) {
                *verif = Box::new(RpcSmartContractWalletVerifier::new(url));
            } else {
                info!("No upgraded chain url for chain {id}, using default.");
            };
        });
        self
    }
}

#[async_trait]
impl SmartContractSignatureVerifier for MultiSmartContractSignatureVerifier {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        if let Some(verifier) = self.verifiers.get(&account_id.chain_id.to_uppercase()) {
            return verifier
                .is_valid_signature(account_id, hash, signature, block_number)
                .await;
        }

        Err(VerifierError::Provider(ProviderError::CustomError(
            "Verifier not present".to_string(),
        )))
    }
}
