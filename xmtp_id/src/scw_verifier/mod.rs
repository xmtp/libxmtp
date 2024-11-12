mod chain_rpc_verifier;
mod remote_signature_verifier;

use crate::associations::AccountId;
use ethers::{
    providers::{Http, Provider, ProviderError},
    types::{BlockNumber, Bytes},
};
use std::{collections::HashMap, fs, path::Path, sync::Arc};
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
    ApiClient(#[from] xmtp_proto::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("URLs must be preceeded with eip144:")]
    MalformedEipUrl,
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> SmartContractSignatureVerifier for Arc<T>
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
        (**self)
            .is_valid_signature(account_id, hash, signature, block_number)
            .await
    }
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
    pub error: Option<String>,
}

pub struct MultiSmartContractSignatureVerifier {
    verifiers: HashMap<String, Box<dyn SmartContractSignatureVerifier + Send + Sync>>,
}

impl MultiSmartContractSignatureVerifier {
    pub fn new(urls: HashMap<String, url::Url>) -> Result<Self, VerifierError> {
        let verifiers = urls
            .into_iter()
            .map(|(chain_id, url)| {
                Ok::<_, VerifierError>((
                    chain_id,
                    Box::new(RpcSmartContractWalletVerifier::new(url.to_string())?)
                        as Box<dyn SmartContractSignatureVerifier + Send + Sync>,
                ))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

        Ok(Self { verifiers })
    }

    pub fn new_from_env() -> Result<Self, VerifierError> {
        let urls: HashMap<String, Url> = serde_json::from_str(DEFAULT_CHAIN_URLS)?;
        Self::new(urls)?.upgrade()
    }

    pub fn new_from_file(path: impl AsRef<Path>) -> Result<Self, VerifierError> {
        let json = fs::read_to_string(path.as_ref())?;
        let urls: HashMap<String, Url> = serde_json::from_str(&json)?;

        Self::new(urls)
    }

    /// Upgrade the default urls to paid/private/alternative urls if the env vars are present.
    pub fn upgrade(mut self) -> Result<Self, VerifierError> {
        for (id, verifier) in self.verifiers.iter_mut() {
            // TODO: coda - update the chain id env var ids to preceeded with "EIP155_"
            let eip_id = id.split(":").nth(1).ok_or(VerifierError::MalformedEipUrl)?;
            if let Ok(url) = std::env::var(format!("CHAIN_RPC_{eip_id}")) {
                *verifier = Box::new(RpcSmartContractWalletVerifier::new(url)?);
            } else {
                info!("No upgraded chain url for chain {id}, using default.");
            };
        }

        #[cfg(feature = "test-utils")]
        if let Ok(url) = std::env::var("ANVIL_URL") {
            info!("Adding anvil to the verifiers: {url}");
            self.verifiers.insert(
                "eip155:31337".to_string(),
                Box::new(RpcSmartContractWalletVerifier::new(url)?),
            );
        }

        Ok(self)
    }

    pub fn add_verifier(&mut self, id: String, url: String) -> Result<(), VerifierError> {
        self.verifiers
            .insert(id, Box::new(RpcSmartContractWalletVerifier::new(url)?));
        Ok(())
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
