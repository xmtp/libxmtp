use alloy::primitives::{BlockNumber, Bytes};
use lru::LruCache;
use parking_lot::Mutex;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::num::NonZeroUsize;
use xmtp_id::associations::AccountId;
use xmtp_id::scw_verifier::{SmartContractSignatureVerifier, ValidationResponse, VerifierError};

/// All verification parameters needed for correct cache isolation.
/// Built for readability, then hashed to a fixed-size u64 for storage
/// so the LRU cache doesn't retain full objects in memory.
/// See: https://github.com/xmtp/libxmtp/issues/3393
#[derive(Clone, Hash, PartialEq, Eq)]
struct CacheKeyInput {
    chain_id: String,
    account_address: String,
    hash: [u8; 32],
    signature: Bytes,
    block_number: Option<BlockNumber>,
}

type CacheKey = u64;

fn build_cache_key(input: &CacheKeyInput) -> CacheKey {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
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
        let cache_key = build_cache_key(&CacheKeyInput {
            chain_id: account_id.get_chain_id().to_string(),
            account_address: account_id.get_account_address().to_string(),
            hash,
            signature: signature.clone(),
            block_number,
        });

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
        let key1: CacheKey = 0;
        let key2: CacheKey = 1;

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

        cache.put(key1, val1.clone());
        let response = cache.get(&key1).unwrap();

        // key1 is correctly cached
        assert_eq!(response.is_valid, val1.is_valid);
        assert_eq!(response.block_number, val1.block_number);

        cache.put(key2, val2.clone());

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
        let hash = [0u8; 32];
        let base = CacheKeyInput {
            chain_id: "eip155:1".into(),
            account_address: "0xaaa".into(),
            hash,
            signature: Bytes::from(vec![1]),
            block_number: Some(100),
        };

        // Different account_address -> different cache key
        let diff_account = CacheKeyInput {
            account_address: "0xbbb".into(),
            ..base.clone()
        };
        assert_ne!(build_cache_key(&base), build_cache_key(&diff_account));

        // Different block_number -> different cache key
        let diff_block = CacheKeyInput {
            block_number: Some(200),
            ..base.clone()
        };
        assert_ne!(build_cache_key(&base), build_cache_key(&diff_block));

        // Different signature -> different cache key
        let diff_sig = CacheKeyInput {
            signature: Bytes::from(vec![2]),
            ..base.clone()
        };
        assert_ne!(build_cache_key(&base), build_cache_key(&diff_sig));

        // Different hash -> different cache key
        let diff_hash = CacheKeyInput {
            hash: [1u8; 32],
            ..base.clone()
        };
        assert_ne!(build_cache_key(&base), build_cache_key(&diff_hash));

        // Same params -> same cache key
        let same = CacheKeyInput { ..base.clone() };
        assert_eq!(build_cache_key(&base), build_cache_key(&same));

        // None vs Some(0) block_number -> different cache key
        let none_block = CacheKeyInput {
            block_number: None,
            ..base.clone()
        };
        let zero_block = CacheKeyInput {
            block_number: Some(0),
            ..base.clone()
        };
        assert_ne!(build_cache_key(&none_block), build_cache_key(&zero_block));
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
