mod chain_rpc_verifier;
mod remote_signature_verifier;
mod cache_key;
use crate::associations::AccountId;
use ethers::{
    providers::{Http, Provider, ProviderError},
    types::{BlockNumber, Bytes},
};
use std::{collections::HashMap, fs, num::NonZeroUsize, path::Path, sync::Arc};
use thiserror::Error;
use tracing::info;
use url::Url;
use lru::LruCache;
use tokio::sync::Mutex;
use cache_key::CacheKey;
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
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("URLs must be preceeded with eip144:")]
    MalformedEipUrl,
    #[error(transparent)]
    Api(#[from] xmtp_api::Error),
    #[error("invalid cache size: {0}")]
    InvalidCacheSize(usize),
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

#[derive(Clone)]
pub struct ValidationResponse {
    pub is_valid: bool,
    pub block_number: Option<u64>,
    pub error: Option<String>,
}

pub struct MultiSmartContractSignatureVerifier {
    verifiers: HashMap<String, Box<dyn SmartContractSignatureVerifier + Send + Sync>>,
    cache: Mutex<LruCache<CacheKey, ValidationResponse>>,
}

impl MultiSmartContractSignatureVerifier {
    pub fn new(urls: HashMap<String, url::Url>, cache_size: usize) -> Result<Self, VerifierError> {
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

        let cache_size = NonZeroUsize::new(cache_size)
            .ok_or(VerifierError::InvalidCacheSize(cache_size))?;

        Ok(Self { 
            verifiers, 
            cache: Mutex::new(LruCache::new(cache_size))
        })
    }

    pub fn new_from_env(cache_size: usize) -> Result<Self, VerifierError> {
        let urls: HashMap<String, Url> = serde_json::from_str(DEFAULT_CHAIN_URLS)?;
        Self::new(urls, cache_size)?.upgrade()
    }

    pub fn new_from_file(path: impl AsRef<Path>, cache_size: usize) -> Result<Self, VerifierError> {
        let json = fs::read_to_string(path.as_ref())?;
        let urls: HashMap<String, Url> = serde_json::from_str(&json)?;

        Self::new(urls, cache_size)
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
        let key = CacheKey::new(&account_id, hash, &signature, block_number);

        if let Some(cached_response) = {
            let mut cache = self.cache.lock().await;
            cache.get(&key).cloned()
        } {
            return Ok(cached_response);
        }

        let response = if let Some(verifier) = self.verifiers.get(&account_id.chain_id) {
            verifier
                .is_valid_signature(account_id, hash, signature, block_number)
                .await
        } else {
            Err(VerifierError::Provider(ProviderError::CustomError(
                "Verifier not present".to_string(),
            )))
        }?;

        let mut cache = self.cache.lock().await;
        cache.put(key, response.clone());

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_eviction() {
        let mut cache: LruCache<CacheKey, ValidationResponse> = LruCache::new(NonZeroUsize::new(1).unwrap());

        let account_id1 = AccountId::new(String::from("chain1"), String::from("account1"));
        let account_id2 = AccountId::new(String::from("chain1"), String::from("account2"));
        let hash = [0u8; 32];
        let bytes = Bytes::from(vec![1, 2, 3]);
        let block_number = Some(BlockNumber::Number(1.into()));

        let key1 = CacheKey::new(&account_id1, hash, &bytes, block_number);
        let key2 = CacheKey::new(&account_id2, hash, &bytes, block_number);
        assert_ne!(key1, key2);

        let val1: ValidationResponse = ValidationResponse { is_valid: true, block_number: Some(1), error: None };
        let val2: ValidationResponse = ValidationResponse { is_valid: true, block_number: Some(2), error: None };

        cache.put(key1.clone(), val1.clone());
        let response = cache.get(&key1).unwrap();
        assert_eq!(response.is_valid, val1.is_valid);
        assert_eq!(response.block_number, val1.block_number);

        cache.put(key2.clone(), val2.clone());
        assert!(cache.get(&key1).is_none());

        // And key2 is correctly cached.
        let response2 = cache.get(&key2).unwrap();
        assert_eq!(response2.is_valid, val2.is_valid);
        assert_eq!(response2.block_number, val2.block_number);
    }

    #[test]
    fn test_invalid_cache_size() {
        let urls: HashMap<String, url::Url> = HashMap::new();
        let result = MultiSmartContractSignatureVerifier::new(urls, 0);
        assert!(result.is_err());
        if let Err(VerifierError::InvalidCacheSize(size)) = result {
            assert_eq!(size, 0);
        } else {
            panic!("Expected an InvalidCacheSize error.");
        }
    }

    #[tokio::test]
    async fn test_missing_verifier() {
        //
        let verifiers = HashMap::new();
        let multi_verifier = MultiSmartContractSignatureVerifier {
            verifiers,
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(1).unwrap())),
        };

        let account_id = AccountId::new("missing".to_string(), "account1".to_string());
        let hash = [0u8; 32];
        let signature = Bytes::from(vec![1, 2, 3]);
        let block_number = Some(BlockNumber::Number(1.into()));

        let result = multi_verifier
            .is_valid_signature(account_id, hash, signature, block_number)
            .await;
        assert!(result.is_err());

        match result {
            Err(VerifierError::Provider(provider_error)) => {
                assert_eq!(
                    provider_error.to_string(),
                    "custom error: Verifier not present"
                );
            }
            _ => {
                panic!("Expected a VerifierError::Provider error.");
            }
        }
    }
}
