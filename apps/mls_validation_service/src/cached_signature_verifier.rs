use lru::LruCache;
use parking_lot::Mutex;
use std::num::NonZeroUsize;

use alloy::primitives::{BlockNumber, Bytes};
use xmtp_id::associations::AccountId;
use xmtp_id::scw_verifier::{SmartContractSignatureVerifier, ValidationResponse, VerifierError};

/// Cache key that incorporates all verification parameters to prevent
/// cross-account cache poisoning and block-number mismatch attacks.
/// See: https://github.com/xmtp/libxmtp/issues/3393
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct CacheKey {
    chain_id: String,
    account_address: String,
    hash: [u8; 32],
    signature: Bytes,
    block_number: Option<BlockNumber>,
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
    pub fn new(
        verifier: impl SmartContractSignatureVerifier + 'static,
        cache_size: NonZeroUsize,
    ) -> Result<Self, VerifierError> {
        Ok(Self {
            verifier: Box::new(verifier),
            cache: Mutex::new(LruCache::new(cache_size)),
        })
    }
}

#[xmtp_common::async_trait]
impl SmartContractSignatureVerifier for CachedSmartContractSignatureVerifier {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        let cache_key = CacheKey {
            chain_id: account_id.get_chain_id().to_string(),
            account_address: account_id.get_account_address().to_string(),
            hash,
            signature: signature.clone(),
            block_number,
        };

        if let Some(cached_response) = {
            let mut cache = self.cache.lock();
            cache.get(&cache_key).cloned()
        } {
            return Ok(cached_response);
        }

        let response = self
            .verifier
            .is_valid_signature(account_id, hash, signature, block_number)
            .await?;

        let mut cache = self.cache.lock();
        cache.put(cache_key, response.clone());

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::dyn_abi::SolType;
    use alloy::primitives::map::HashMap;
    use alloy::primitives::{B256, U256};
    use alloy::providers::Provider;
    use alloy::signers::Signer;
    use std::time::Duration;
    use xmtp_id::scw_verifier::{
        MultiSmartContractSignatureVerifier, SmartContractSignatureVerifier, ValidationResponse,
        VerifierError,
    };
    use xmtp_id::utils::test::{SignatureWithNonce, SmartWalletContext, docker_smart_wallet};

    #[rstest::rstest]
    #[xmtp_common::timeout(Duration::from_secs(60))]
    #[tokio::test]
    async fn test_is_valid_signature(#[future] docker_smart_wallet: SmartWalletContext) {
        let SmartWalletContext {
            factory,
            owner0: owner,
            sw,
            sw_address,
            ..
        } = docker_smart_wallet.await;
        let chain_id = factory.provider().get_chain_id().await.unwrap();
        let hash = B256::random();
        let replay_safe_hash = sw.replaySafeHash(hash).call().await.unwrap();
        let signature = owner.sign_hash(&replay_safe_hash).await.unwrap();
        let signature =
            SignatureWithNonce::abi_encode(&(U256::from(0), signature.as_bytes().to_vec()));

        // Create the verifiers map
        let mut verifiers = HashMap::new();
        verifiers.insert("eip155:31337".to_string(), factory.provider().clone());

        // Create the cached verifier
        let verifier = CachedSmartContractSignatureVerifier::new(
            MultiSmartContractSignatureVerifier::new_providers(verifiers).unwrap(),
            NonZeroUsize::new(5).unwrap(),
        )
        .unwrap();

        let account_id = AccountId::new_evm(chain_id, format!("{sw_address:?}"));

        // Testing ERC-6492 signatures with deployed ERC-1271.
        assert!(
            verifier
                .is_valid_signature(account_id.clone(), *hash, signature.clone().into(), None)
                .await
                .unwrap()
                .is_valid
        );

        assert!(
            !verifier
                .is_valid_signature(
                    account_id.clone(),
                    B256::random().into(),
                    signature.into(),
                    None
                )
                .await
                .unwrap()
                .is_valid
        );

        // Testing if EOA wallet signature is valid on ERC-6492
        let signature = owner.sign_hash(&hash).await.unwrap();
        let owner_account_id = AccountId::new_evm(chain_id, format!("{:?}", owner.address()));
        assert!(
            verifier
                .is_valid_signature(
                    owner_account_id.clone(),
                    *hash,
                    signature.as_bytes().into(),
                    None
                )
                .await
                .unwrap()
                .is_valid
        );

        assert!(
            !verifier
                .is_valid_signature(
                    owner_account_id,
                    B256::random().into(),
                    signature.as_bytes().into(),
                    None
                )
                .await
                .unwrap()
                .is_valid
        );
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let mut cache: LruCache<CacheKey, ValidationResponse> =
            LruCache::new(NonZeroUsize::new(1).unwrap());
        let key1 = CacheKey {
            chain_id: "eip155:1".to_string(),
            account_address: "0xabc".to_string(),
            hash: [0u8; 32],
            signature: Bytes::from(vec![1, 2, 3]),
            block_number: Some(1),
        };
        let key2 = CacheKey {
            chain_id: "eip155:1".to_string(),
            account_address: "0xdef".to_string(),
            hash: [1u8; 32],
            signature: Bytes::from(vec![4, 5, 6]),
            block_number: Some(2),
        };

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

        // key1 is correctly cached
        assert_eq!(response.is_valid, val1.is_valid);
        assert_eq!(response.block_number, val1.block_number);

        cache.put(key2.clone(), val2.clone());

        // key1 is evicted, shouldn't exist
        assert!(cache.get(&key1).is_none());

        // And key2 is correctly cached
        let response2 = cache.get(&key2).unwrap();
        assert_eq!(response2.is_valid, val2.is_valid);
        assert_eq!(response2.block_number, val2.block_number);
    }

    /// Verifies that same hash with different account/signature/block produces different cache keys.
    /// Regression test for https://github.com/xmtp/libxmtp/issues/3393
    #[tokio::test]
    async fn test_cache_key_includes_all_params() {
        let mut cache: LruCache<CacheKey, ValidationResponse> =
            LruCache::new(NonZeroUsize::new(10).unwrap());
        let hash = [0u8; 32];

        // Same hash, different account_address
        let key_account_a = CacheKey {
            chain_id: "eip155:1".to_string(),
            account_address: "0xaaa".to_string(),
            hash,
            signature: Bytes::from(vec![1]),
            block_number: Some(100),
        };
        let key_account_b = CacheKey {
            chain_id: "eip155:1".to_string(),
            account_address: "0xbbb".to_string(),
            hash,
            signature: Bytes::from(vec![1]),
            block_number: Some(100),
        };

        cache.put(
            key_account_a.clone(),
            ValidationResponse {
                is_valid: true,
                block_number: Some(100),
                error: None,
            },
        );

        // Different account should NOT hit the cache
        assert!(cache.get(&key_account_b).is_none());
        // Same account should hit the cache
        assert!(cache.get(&key_account_a).is_some());

        // Same hash, different block_number
        let key_block_1 = CacheKey {
            chain_id: "eip155:1".to_string(),
            account_address: "0xaaa".to_string(),
            hash,
            signature: Bytes::from(vec![1]),
            block_number: Some(100),
        };
        let key_block_2 = CacheKey {
            chain_id: "eip155:1".to_string(),
            account_address: "0xaaa".to_string(),
            hash,
            signature: Bytes::from(vec![1]),
            block_number: Some(200),
        };

        cache.put(
            key_block_1.clone(),
            ValidationResponse {
                is_valid: true,
                block_number: Some(100),
                error: None,
            },
        );

        // Different block_number should NOT hit the cache
        assert!(cache.get(&key_block_2).is_none());
        assert!(cache.get(&key_block_1).is_some());

        // Same hash, different signature
        let key_sig_a = CacheKey {
            chain_id: "eip155:1".to_string(),
            account_address: "0xaaa".to_string(),
            hash,
            signature: Bytes::from(vec![1, 2, 3]),
            block_number: Some(100),
        };
        let key_sig_b = CacheKey {
            chain_id: "eip155:1".to_string(),
            account_address: "0xaaa".to_string(),
            hash,
            signature: Bytes::from(vec![4, 5, 6]),
            block_number: Some(100),
        };

        cache.put(
            key_sig_a.clone(),
            ValidationResponse {
                is_valid: true,
                block_number: Some(100),
                error: None,
            },
        );

        // Different signature should NOT hit the cache
        assert!(cache.get(&key_sig_b).is_none());
        assert!(cache.get(&key_sig_a).is_some());
    }

    #[tokio::test]
    async fn test_missing_verifier() {
        let verifiers = std::collections::HashMap::new();
        let multi_verifier = MultiSmartContractSignatureVerifier::new(verifiers).unwrap();
        let cached_verifier = CachedSmartContractSignatureVerifier::new(
            multi_verifier,
            NonZeroUsize::new(1).unwrap(),
        )
        .unwrap();

        let account_id = AccountId::new("missing".to_string(), "account1".to_string());
        let hash = [0u8; 32];
        let signature = Bytes::from(vec![1, 2, 3]);
        let block_number = Some(1);

        let result: Result<_, VerifierError> = cached_verifier
            .is_valid_signature(account_id, hash, signature, block_number)
            .await;
        assert!(result.is_err());
        assert!(matches!(result, Err(VerifierError::NoVerifier(_))));
    }
}
