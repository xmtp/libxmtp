use crate::associations::AccountId;
use crate::scw_verifier::{SmartContractSignatureVerifier, ValidationResponse, VerifierError};
use alloy::primitives::{BlockNumber, Bytes, keccak256};
use lru::LruCache;
use parking_lot::Mutex;
use std::num::NonZeroUsize;

/// 32-byte cache key derived from all verification parameters via keccak256.
/// Prevents cross-account cache poisoning while keeping memory constant per entry.
/// See: https://github.com/xmtp/libxmtp/issues/3393
type CacheKey = [u8; 32];

/// Build a collision-resistant cache key by hashing all verification parameters.
/// All fields are borrowed — no cloning required.
fn build_cache_key(
    account_id: &AccountId,
    hash: &[u8; 32],
    signature: &[u8],
    block_number: Option<BlockNumber>,
) -> CacheKey {
    let chain_id = account_id.get_chain_id().as_bytes();
    let account_address = account_id.get_account_address().as_bytes();
    let bn_bytes = block_number.map(|bn| bn.to_be_bytes());

    // Pre-allocate: 4-byte lengths for 3 variable fields + field data + 1 tag + 8 optional
    let capacity = 4 + chain_id.len() + 4 + account_address.len() + 32 + 4 + signature.len() + 9;
    let mut buf = Vec::with_capacity(capacity);

    // Length-prefix variable-length fields for unambiguous encoding
    buf.extend_from_slice(&(chain_id.len() as u32).to_be_bytes());
    buf.extend_from_slice(chain_id);
    buf.extend_from_slice(&(account_address.len() as u32).to_be_bytes());
    buf.extend_from_slice(account_address);
    buf.extend_from_slice(hash);
    buf.extend_from_slice(&(signature.len() as u32).to_be_bytes());
    buf.extend_from_slice(signature);
    match bn_bytes {
        Some(bytes) => {
            buf.push(0x01);
            buf.extend_from_slice(&bytes);
        }
        None => buf.push(0x00),
    }
    *keccak256(&buf)
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
        // Latest state can change without changing the request.
        if block_number.is_none() {
            return self
                .verifier
                .is_valid_signature(account_id, hash, signature, None)
                .await;
        }
        let cache_key = build_cache_key(&account_id, &hash, &signature, block_number);

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
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct CountingVerifier {
        calls: Arc<AtomicUsize>,
        valid: bool,
        error: bool,
    }
    #[xmtp_common::async_trait]
    impl SmartContractSignatureVerifier for CountingVerifier {
        async fn is_valid_signature(
            &self,
            _: AccountId,
            _: [u8; 32],
            _: Bytes,
            block: Option<BlockNumber>,
        ) -> Result<ValidationResponse, VerifierError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if self.error {
                return Err(VerifierError::NoVerifier("eip155:1".into()));
            }
            Ok(ValidationResponse {
                is_valid: self.valid,
                block_number: Some(block.unwrap_or(call as u64)),
                error: None,
            })
        }
    }
    #[xmtp_common::test(unwrap_try = true)]
    async fn cache_preserves_numbered_verdicts_and_bypasses_latest() {
        for valid in [true, false] {
            let calls = Arc::new(AtomicUsize::new(0));
            let cache = CachedSmartContractSignatureVerifier::new(
                CountingVerifier {
                    calls: calls.clone(),
                    valid,
                    error: false,
                },
                NonZeroUsize::new(1).unwrap(),
            )?;
            let account = AccountId::new_evm(1, "0xaaa".into());
            for _ in 0..2 {
                let result = cache
                    .is_valid_signature(account.clone(), [0; 32], Bytes::new(), Some(1))
                    .await?;
                assert_eq!(result.is_valid, valid);
            }
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            let first = cache
                .is_valid_signature(account.clone(), [0; 32], Bytes::new(), None)
                .await?;
            let second = cache
                .is_valid_signature(account.clone(), [0; 32], Bytes::new(), None)
                .await?;
            assert_ne!(first.block_number, second.block_number);
            assert_eq!(calls.load(Ordering::SeqCst), 3);
            cache
                .is_valid_signature(account.clone(), [0; 32], Bytes::new(), Some(2))
                .await?;
            cache
                .is_valid_signature(account, [0; 32], Bytes::new(), Some(1))
                .await?;
            assert_eq!(calls.load(Ordering::SeqCst), 5);
        }
    }
    #[xmtp_common::test(unwrap_try = true)]
    async fn verifier_errors_are_never_cached() {
        let calls = Arc::new(AtomicUsize::new(0));
        let cache = CachedSmartContractSignatureVerifier::new(
            CountingVerifier {
                calls: calls.clone(),
                valid: false,
                error: true,
            },
            NonZeroUsize::new(1).unwrap(),
        )?;
        for _ in 0..2 {
            assert!(matches!(
                cache
                    .is_valid_signature(
                        AccountId::new_evm(1, "0xaaa".into()),
                        [0; 32],
                        Bytes::new(),
                        Some(1)
                    )
                    .await,
                Err(VerifierError::NoVerifier(_))
            ));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
    #[xmtp_common::test(unwrap_try = true)]
    fn cache_key_binds_every_parameter() {
        let account = AccountId::new("eip155:1".into(), "0xaaa".into());
        let key = build_cache_key(&account, &[0; 32], &[1], Some(0));
        assert_ne!(
            key,
            build_cache_key(
                &AccountId::new("eip155:2".into(), "0xaaa".into()),
                &[0; 32],
                &[1],
                Some(0)
            )
        );
        assert_ne!(
            key,
            build_cache_key(
                &AccountId::new("eip155:1".into(), "0xbbb".into()),
                &[0; 32],
                &[1],
                Some(0)
            )
        );
        assert_ne!(key, build_cache_key(&account, &[1; 32], &[1], Some(0)));
        assert_ne!(key, build_cache_key(&account, &[0; 32], &[2], Some(0)));
        assert_ne!(key, build_cache_key(&account, &[0; 32], &[1], Some(1)));
        assert_ne!(key, build_cache_key(&account, &[0; 32], &[1], None));
        let left = AccountId::new("ab".into(), "c".into());
        let right = AccountId::new("a".into(), "bc".into());
        assert_ne!(
            build_cache_key(&left, &[0; 32], &[1], Some(0)),
            build_cache_key(&right, &[0; 32], &[1], Some(0))
        );
    }
}
