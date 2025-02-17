use lru::LruCache;
use parking_lot::Mutex;
use std::num::NonZeroUsize;

use ethers::types::{BlockNumber, Bytes};
use xmtp_id::associations::AccountId;
use xmtp_id::scw_verifier::{SmartContractSignatureVerifier, ValidationResponse, VerifierError};

type CacheKey = [u8; 32];

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
        if let Some(cached_response) = {
            let mut cache = self.cache.lock();
            cache.get(&hash).cloned()
        } {
            return Ok(cached_response);
        }

        let response = self
            .verifier
            .is_valid_signature(account_id, hash, signature, block_number)
            .await?;

        let mut cache = self.cache.lock();
        cache.put(hash, response.clone());

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::{
        abi::{self, Token},
        middleware::MiddlewareBuilder,
        providers::{Http, Middleware, Provider},
        signers::{LocalWallet, Signer as _},
        types::{H256, U256},
    };
    use std::{collections::HashMap, sync::Arc};
    use xmtp_id::scw_verifier::{
        MultiSmartContractSignatureVerifier, SmartContractSignatureVerifier, ValidationResponse,
        VerifierError,
    };
    use xmtp_id::utils::test::{with_smart_contracts, CoinbaseSmartWallet};

    #[tokio::test]
    async fn test_is_valid_signature() {
        with_smart_contracts(|anvil, _provider, client, smart_contracts| async move {
            let owner: LocalWallet = anvil.keys()[0].clone().into();
            let owners_addresses = vec![Bytes::from(H256::from(owner.address()).0.to_vec())];
            let factory = smart_contracts.coinbase_smart_wallet_factory();
            let nonce = U256::from(0);
            let smart_wallet_address = factory
                .get_address(owners_addresses.clone(), nonce)
                .await
                .unwrap();
            let contract_call = factory.create_account(owners_addresses.clone(), nonce);
            let pending_tx = contract_call.send().await.unwrap();
            pending_tx.await.unwrap();

            // Check that the smart contract is deployed
            let provider: Provider<Http> = Provider::new(anvil.endpoint().parse().unwrap());
            let code = provider.get_code(smart_wallet_address, None).await.unwrap();
            assert!(!code.is_empty());

            // Generate the signature for coinbase smart wallet
            let smart_wallet = CoinbaseSmartWallet::new(
                smart_wallet_address,
                Arc::new(client.with_signer(owner.clone().with_chain_id(anvil.chain_id()))),
            );
            let hash: [u8; 32] = H256::random().into();
            let replay_safe_hash = smart_wallet.replay_safe_hash(hash).call().await.unwrap();
            let signature = owner.sign_hash(replay_safe_hash.into()).unwrap();
            let signature: Bytes = abi::encode(&[Token::Tuple(vec![
                Token::Uint(U256::from(0)),
                Token::Bytes(signature.to_vec()),
            ])])
            .into();

            // Create the verifiers map
            let mut verifiers = HashMap::new();
            verifiers.insert(
                "eip155:31337".to_string(),
                anvil.endpoint().parse().unwrap(),
            );

            // Create the cached verifier
            let verifier = CachedSmartContractSignatureVerifier::new(
                MultiSmartContractSignatureVerifier::new(verifiers).unwrap(),
                NonZeroUsize::new(5).unwrap(),
            )
            .unwrap();

            let account_id =
                AccountId::new_evm(anvil.chain_id(), format!("{:?}", smart_wallet_address));

            // Testing ERC-6492 signatures with deployed ERC-1271.
            assert!(
                verifier
                    .is_valid_signature(account_id.clone(), hash, signature.clone(), None)
                    .await
                    .unwrap()
                    .is_valid
            );

            assert!(
                !verifier
                    .is_valid_signature(account_id.clone(), H256::random().into(), signature, None)
                    .await
                    .unwrap()
                    .is_valid
            );

            // Testing if EOA wallet signature is valid on ERC-6492
            let signature = owner.sign_hash(hash.into()).unwrap();
            let owner_account_id =
                AccountId::new_evm(anvil.chain_id(), format!("{:?}", owner.address()));
            assert!(
                verifier
                    .is_valid_signature(
                        owner_account_id.clone(),
                        hash,
                        signature.to_vec().into(),
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
                        H256::random().into(),
                        signature.to_vec().into(),
                        None
                    )
                    .await
                    .unwrap()
                    .is_valid
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let mut cache: LruCache<CacheKey, ValidationResponse> =
            LruCache::new(NonZeroUsize::new(1).unwrap());
        let key1 = [0u8; 32];
        let key2 = [1u8; 32];

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

    #[tokio::test]
    async fn test_missing_verifier() {
        //
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
