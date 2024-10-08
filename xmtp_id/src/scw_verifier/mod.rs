mod chain_rpc_verifier;
mod remote_signature_verifier;

use std::{collections::HashMap, fs, path::Path, str::FromStr};

use crate::associations::AccountId;
use ethers::{
    providers::{Http, Provider, ProviderError},
    types::{BlockNumber, Bytes},
};
use thiserror::Error;
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
    ApiClient(#[from] xmtp_proto::api_client::Error),
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait SmartContractSignatureVerifier: Send + Sync {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError>;
}

#[cfg(target_arch = "wasm32")]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait SmartContractSignatureVerifier {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError>;
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<T> SmartContractSignatureVerifier for &T
where
    T: SmartContractSignatureVerifier,
{
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        (*self)
            .is_valid_signature(account_id, hash, signature, block_number)
            .await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<T> SmartContractSignatureVerifier for Box<T>
where
    T: SmartContractSignatureVerifier + ?Sized,
{
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

pub struct ValidationResponse {
    pub is_valid: bool,
    pub block_number: Option<u64>,
}

pub struct MultiSmartContractSignatureVerifier {
    verifiers: HashMap<String, Box<dyn SmartContractSignatureVerifier + Send + Sync>>,
}

impl MultiSmartContractSignatureVerifier {
    pub fn new(urls: HashMap<String, url::Url>) -> Self {
        let verifiers = urls
            .into_iter()
            .map(|(chain_id, url)| {
                (
                    chain_id,
                    Box::new(RpcSmartContractWalletVerifier::new(url.to_string()))
                        as Box<dyn SmartContractSignatureVerifier + Send + Sync>,
                )
            })
            .collect();

        Self { verifiers }
    }

    pub fn new_from_file(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();

        let file_str;
        let json = if path.exists() {
            file_str = fs::read_to_string(path).unwrap_or_else(|_| panic!("{path:?} is missing"));
            &file_str
        } else {
            DEFAULT_CHAIN_URLS
        };

        let json: HashMap<String, String> =
            serde_json::from_str(json).unwrap_or_else(|_| panic!("{path:?} is malformatted"));

        let urls = json
            .into_iter()
            .map(|(id, url)| {
                (
                    id,
                    Url::from_str(&url)
                        .unwrap_or_else(|_| panic!("unable to parse url in {path:?} ({url})")),
                )
            })
            .collect();

        Self::new(urls)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl SmartContractSignatureVerifier for MultiSmartContractSignatureVerifier {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        if let Some(verifier) = self.verifiers.get(&account_id.chain_id) {
            return verifier
                .is_valid_signature(account_id, hash, signature, block_number)
                .await;
        }

        Err(VerifierError::Provider(ProviderError::CustomError(
            "Verifier not present".to_string(),
        )))
    }
}
