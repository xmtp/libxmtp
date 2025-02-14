use std::num::NonZeroUsize;
use tokio::sync::Mutex;
use lru::LruCache;

use ethers::types::{BlockNumber, Bytes};
use xmtp_id::associations::AccountId;
use xmtp_id::scw_verifier::{SmartContractSignatureVerifier, ValidationResponse,VerifierError};


#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct CacheKey {
    pub address: String,
    pub chain_id: String,
    pub hash: [u8; 32],
    pub signature: Vec<u8>,
    pub block_number: Option<u64>,
}

impl CacheKey {
    pub fn new(
        account_id: &AccountId,
        hash: [u8; 32],
        signature: &Bytes,
        block_number: Option<BlockNumber>,
    ) -> Self {
        let block_number_u64 = block_number.and_then(|bn| bn.as_number().map(|n| n.as_u64()));
        let address = account_id.get_account_address().to_string();
        let chain_id = account_id.get_chain_id().to_string();

        Self {
            chain_id,
            address,
            hash,
            signature: signature.to_vec(),
            block_number: block_number_u64,
        }
    }
}

/// A cached smart contract verifier.
///
/// This wraps MultiSmartContractSignatureVerifier (or any other verifier
/// implementing SmartContractSignatureVerifier) and adds an in-memory LRU cache.
pub struct CachedSmartContractSignatureVerifier {
    verifier: Box<dyn SmartContractSignatureVerifier>,
    cache: Mutex<LruCache<CacheKey, ValidationResponse>>,
}

impl CachedSmartContractSignatureVerifier {
    pub fn new(verifier: impl SmartContractSignatureVerifier + 'static, cache_size: usize) -> Result<Self, VerifierError> {
        if cache_size == 0 {
            return Err(VerifierError::InvalidCacheSize(cache_size));
        }
        Ok(Self {
            verifier: Box::new(verifier),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(cache_size).unwrap())),
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl SmartContractSignatureVerifier for CachedSmartContractSignatureVerifier {
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

        let response = self
            .verifier
            .is_valid_signature(account_id, hash, signature, block_number)
            .await?;

        let mut cache = self.cache.lock().await;
        cache.put(key, response.clone());

        Ok(response)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use xmtp_id::scw_verifier::{SmartContractSignatureVerifier, MultiSmartContractSignatureVerifier, ValidationResponse,VerifierError};
    use url::Url;

    #[test]
    fn test_cache_eviction() {
        let mut cache: LruCache<CacheKey, ValidationResponse> =
            LruCache::new(NonZeroUsize::new(1).unwrap());

        let account_id1 = AccountId::new(String::from("chain1"), String::from("account1"));
        let account_id2 = AccountId::new(String::from("chain1"), String::from("account2"));
        let hash = [0u8; 32];
        let bytes = Bytes::from(vec![1, 2, 3]);
        let block_number = Some(BlockNumber::Number(1.into()));

        let key1 = CacheKey::new(&account_id1, hash, &bytes, block_number);
        let key2 = CacheKey::new(&account_id2, hash, &bytes, block_number);
        assert_ne!(key1, key2);

        let val1: ValidationResponse = ValidationResponse {
            is_valid: true,
            block_number: Some(1),
            error: None,
        };
        let val2: ValidationResponse = ValidationResponse {
            is_valid: true,
            block_number: Some(2),
            error: None,
        };

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
        let urls: HashMap<String, Url> = HashMap::new();
        let scw_verifier = MultiSmartContractSignatureVerifier::new(urls)
            .expect("Failed to create MultiSmartContractSignatureVerifier");

        let err = CachedSmartContractSignatureVerifier::new(scw_verifier, 0);
        if let Err(VerifierError::InvalidCacheSize(size)) = err {
            assert_eq!(size, 0);
        } else {
                panic!("Expected a VerifierError::InvalidCacheSize");
            }
    }

    #[tokio::test]
    async fn test_missing_verifier() {
        //
        let verifiers = std::collections::HashMap::new();
        let multi_verifier = MultiSmartContractSignatureVerifier::new(verifiers).unwrap();
        let cached_verifier = CachedSmartContractSignatureVerifier::new(multi_verifier, 1).unwrap();

        let account_id = AccountId::new("missing".to_string(), "account1".to_string());
        let hash = [0u8; 32];
        let signature = Bytes::from(vec![1, 2, 3]);
        let block_number = Some(BlockNumber::Number(1.into()));

        let result = cached_verifier
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
