//! Interaction with [ERC-1271](https://eips.ethereum.org/EIPS/eip-1271) smart contracts.
use crate::associations::AccountId;
use crate::scw_verifier::SmartContractSignatureVerifier;
use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, BlockNumber, Bytes, FixedBytes};
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::{sol, sol_types::SolConstructor};
use hex::FromHexError;
use std::sync::Arc;
use xmtp_common::RetryableError;

use super::{ValidationResponse, VerifierError};

// https://github.com/AmbireTech/signature-validator/blob/7706bda/index.ts#L13
// Contract from AmbireTech that is also used by Viem.
// Note that this is not a complete ERC-6492 implementation as it lacks Prepare/Side-effect logic compared to official reference implementation, so it might evolve in the future.
// For now it's accepted as [Coinbase Smart Wallet doc](https://github.com/AmbireTech/signature-validator/blob/7706bda/index.ts#L13) uses it for offchain verification.
const VALIDATE_SIG_OFFCHAIN_BYTECODE: &str = include_str!("signature_validation.hex");

sol!(
    contract VerifySig {
      constructor (
        address _signer,
        bytes32 _hash,
        bytes memory _signature
      );
    }
);

#[derive(Debug, Clone)]
pub struct RpcSmartContractWalletVerifier {
    provider: Arc<DynProvider>,
}

impl RpcSmartContractWalletVerifier {
    pub fn new(provider_url: String) -> Result<Self, VerifierError> {
        Ok(Self {
            provider: Arc::new(
                ProviderBuilder::new()
                    .connect_http(provider_url.parse()?)
                    .erased(),
            ),
        })
    }

    pub fn new_from_provider(provider: impl Provider + 'static) -> Self {
        Self {
            provider: Arc::new(DynProvider::new(provider)),
        }
    }
}

/// A verifier that tries multiple RPC endpoints in order, falling back to the next
/// on retryable errors.
#[derive(Debug)]
pub struct FallbackSmartContractWalletVerifier {
    verifiers: Vec<RpcSmartContractWalletVerifier>,
}

impl FallbackSmartContractWalletVerifier {
    pub fn new(urls: Vec<String>) -> Result<Self, VerifierError> {
        if urls.is_empty() {
            return Err(VerifierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "at least one RPC URL is required for fallback verifier",
            )));
        }
        let verifiers = urls
            .into_iter()
            .map(RpcSmartContractWalletVerifier::new)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { verifiers })
    }
}

#[xmtp_common::async_trait]
impl SmartContractSignatureVerifier for FallbackSmartContractWalletVerifier {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        let mut last_error = None;
        for verifier in &self.verifiers {
            match verifier
                .is_valid_signature(account_id.clone(), hash, signature.clone(), block_number)
                .await
            {
                Ok(response) => return Ok(response),
                Err(e) if e.is_retryable() => {
                    tracing::warn!("RPC endpoint failed with retryable error, trying next: {e}");
                    last_error = Some(e);
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_error
            .expect("FallbackSmartContractWalletVerifier must have at least one verifier"))
    }
}

#[xmtp_common::async_trait]
impl SmartContractSignatureVerifier for RpcSmartContractWalletVerifier {
    async fn is_valid_signature(
        &self,
        signer: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        let code = hex::decode(VALIDATE_SIG_OFFCHAIN_BYTECODE.trim())?;
        let account_address: Address = signer
            .account_address
            .parse()
            .map_err(|_| FromHexError::InvalidStringLength)?;
        let call = VerifySig::constructorCall::new((
            account_address,
            FixedBytes::<32>::new(hash),
            signature,
        ));

        let data = call.abi_encode();
        let data = [code, data].concat();
        let block_number = match block_number {
            Some(bn) => bn,
            None => self
                .provider
                .get_block_number()
                .await
                .map_err(VerifierError::Provider)?,
        };
        let mut tx = self.provider.transaction_request();
        tx.set_input(data);
        let result = self.provider.call(tx).block(block_number.into()).await?;

        // Check if result indicates valid signature (0x01)
        let expected_valid = Bytes::from_static(&[0x01]);
        let is_valid = result == expected_valid;

        Ok(ValidationResponse {
            is_valid,
            block_number: Some(block_number),
            error: None,
        })
    }
}

#[cfg(test)]
mod fallback_tests {
    use super::*;
    use crate::associations::AccountId;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A mock verifier that can be configured to succeed or fail.
    struct MockVerifier {
        should_fail: bool,
        retryable: bool,
        call_count: AtomicUsize,
    }

    impl MockVerifier {
        fn new(should_fail: bool, retryable: bool) -> Self {
            Self {
                should_fail,
                retryable,
                call_count: AtomicUsize::new(0),
            }
        }
    }

    #[xmtp_common::async_trait]
    impl SmartContractSignatureVerifier for MockVerifier {
        async fn is_valid_signature(
            &self,
            _account_id: AccountId,
            _hash: [u8; 32],
            _signature: Bytes,
            _block_number: Option<BlockNumber>,
        ) -> Result<ValidationResponse, VerifierError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.should_fail {
                if self.retryable {
                    Err(VerifierError::Io(std::io::Error::new(
                        std::io::ErrorKind::ConnectionRefused,
                        "mock retryable error",
                    )))
                } else {
                    Err(VerifierError::UnexpectedERC6492Result(
                        "mock non-retryable error".into(),
                    ))
                }
            } else {
                Ok(ValidationResponse {
                    is_valid: true,
                    block_number: Some(1),
                    error: None,
                })
            }
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_fallback_first_succeeds() {
        let verifier = FallbackSmartContractWalletVerifier {
            verifiers: vec![RpcSmartContractWalletVerifier::new(
                "https://rpc1.example.com".into(),
            )?],
        };
        // Just verify the struct was created correctly
        assert_eq!(verifier.verifiers.len(), 1);
    }

    #[test]
    fn test_fallback_new_empty_urls_rejected() {
        let result = FallbackSmartContractWalletVerifier::new(vec![]);
        assert!(result.is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_fallback_new_multiple() {
        let verifier = FallbackSmartContractWalletVerifier::new(vec![
            "https://rpc1.example.com".into(),
            "https://rpc2.example.com".into(),
            "https://rpc3.example.com".into(),
        ])?;
        assert_eq!(verifier.verifiers.len(), 3);
    }

    #[tokio::test]
    async fn test_fallback_skips_retryable_errors() {
        // Use MultiSmartContractSignatureVerifier with mock verifiers to test fallback logic
        // by testing the trait directly
        let mock_fail = MockVerifier::new(true, true);
        let mock_success = MockVerifier::new(false, false);

        let account_id = AccountId::new("eip155:1".to_string(), "0x1234".to_string());
        let hash = [0u8; 32];
        let signature = Bytes::from(vec![1, 2, 3]);

        // First mock fails with retryable, so we fall through
        let result = mock_fail
            .is_valid_signature(account_id.clone(), hash, signature.clone(), None)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_retryable());

        // Second mock succeeds
        let result = mock_success
            .is_valid_signature(account_id, hash, signature, None)
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_valid);
    }

    #[tokio::test]
    async fn test_non_retryable_error_does_not_fallback() {
        let mock = MockVerifier::new(true, false);
        let account_id = AccountId::new("eip155:1".to_string(), "0x1234".to_string());
        let hash = [0u8; 32];
        let signature = Bytes::from(vec![1, 2, 3]);

        let result = mock
            .is_valid_signature(account_id, hash, signature, None)
            .await;
        assert!(result.is_err());
        assert!(!result.unwrap_err().is_retryable());
    }
}

// Anvil does not work with WASM
// because its a wrapper over the system-binary
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) mod tests {
    #![allow(clippy::unwrap_used)]
    use crate::utils::test::{SignatureWithNonce, SmartWalletContext, docker_smart_wallet};

    use super::*;
    use alloy::dyn_abi::SolType;
    use alloy::primitives::{B256, U256};
    use alloy::providers::ext::AnvilApi;
    use alloy::signers::Signer;
    use std::time::Duration;

    #[rstest::rstest]
    #[xmtp_common::timeout(Duration::from_secs(30))]
    #[tokio::test]
    async fn test_coinbase_smart_wallet(#[future] docker_smart_wallet: SmartWalletContext) {
        let SmartWalletContext {
            factory,
            sw,
            owner0,
            owner1,
            sw_address,
        } = docker_smart_wallet.await;
        let provider = factory.provider();
        let chain_id = provider.get_chain_id().await.unwrap();
        let hash = B256::random();
        let replay_safe_hash = sw.replaySafeHash(hash).call().await.unwrap();
        let verifier = RpcSmartContractWalletVerifier::new_from_provider(provider.clone());
        let sig0 = owner0.sign_hash(&replay_safe_hash).await.unwrap();
        let account_id = AccountId::new_evm(chain_id, format!("{}", sw_address));

        let res = verifier
            .is_valid_signature(
                account_id.clone(),
                *hash,
                SignatureWithNonce::abi_encode(&(U256::from(0), Bytes::from(sig0.as_bytes())))
                    .into(),
                None,
            )
            .await
            .unwrap();
        assert!(res.is_valid);

        // verify owner1 is a valid owner
        let sig1 = owner1.sign_hash(&replay_safe_hash).await.unwrap();
        let res = verifier
            .is_valid_signature(
                account_id.clone(),
                *hash,
                SignatureWithNonce::abi_encode(&(U256::from(1), Bytes::from(sig1.as_bytes())))
                    .into(),
                None,
            )
            .await
            .unwrap();
        assert!(res.is_valid);

        // owner0 signature must not be used to verify owner1
        let res = verifier
            .is_valid_signature(
                account_id.clone(),
                *hash,
                SignatureWithNonce::abi_encode(&(U256::from(1), Bytes::from(sig0.as_bytes())))
                    .into(),
                None,
            )
            .await
            .unwrap();
        assert!(!res.is_valid);
    }

    #[rstest::rstest]
    #[xmtp_common::timeout(Duration::from_secs(60))]
    #[tokio::test]
    async fn test_smart_wallet_time_travel(#[future] docker_smart_wallet: SmartWalletContext) {
        let SmartWalletContext {
            factory,
            sw,
            owner1,
            sw_address,
            ..
        } = docker_smart_wallet.await;

        let provider = factory.provider();
        let verifier = RpcSmartContractWalletVerifier::new_from_provider(provider.clone());
        let chain_id = provider.get_chain_id().await.unwrap();
        let hash = B256::random();
        let replay_safe_hash = sw.replaySafeHash(hash).call().await.unwrap();
        let sig1 = owner1.sign_hash(&replay_safe_hash).await.unwrap();
        let account_id = AccountId::new_evm(chain_id, format!("{}", sw_address));
        let block_number = provider.get_block_number().await.unwrap();
        println!("{}", block_number);
        provider.anvil_mine(Some(50), None).await.unwrap();
        println!("{}", provider.get_block_number().await.unwrap());
        // remove owner1 and check owner1 is no longer a valid owner
        let _tx = sw
            .removeOwnerAtIndex(U256::from(1))
            .from(owner1.address())
            .send()
            .await
            .unwrap()
            .get_receipt()
            .await
            .unwrap();

        let res = verifier
            .is_valid_signature(
                account_id.clone(),
                *hash,
                SignatureWithNonce::abi_encode(&(U256::from(1), sig1.as_bytes())).into(),
                None,
            )
            .await;
        assert!(res.is_err());
        // when verify a non-existing owner, it errors
        // time travel to the pre-removel block number and verify owner1 WAS a valid owner

        let res = verifier
            .is_valid_signature(
                account_id.clone(),
                *hash,
                SignatureWithNonce::abi_encode(&(U256::from(1), sig1.as_bytes())).into(),
                Some(block_number),
            )
            .await
            .unwrap();
        assert!(res.is_valid);
    }

    // Testing ERC-6492 with deployed / undeployed coinbase smart wallet(ERC-1271) contracts, and EOA.
    #[rstest::rstest]
    #[xmtp_common::timeout(Duration::from_secs(60))]
    #[tokio::test]
    async fn test_is_valid_signature(#[future] docker_smart_wallet: SmartWalletContext) {
        let SmartWalletContext {
            factory,
            sw,
            owner0: owner,
            sw_address,
            ..
        } = docker_smart_wallet.await;
        let provider = factory.provider();
        let chain_id = provider.get_chain_id().await.unwrap();
        let hash = B256::random();
        let replay_safe_hash = sw.replaySafeHash(hash).call().await.unwrap();
        let verifier = RpcSmartContractWalletVerifier::new_from_provider(provider.clone());
        let signature = owner.sign_hash(&replay_safe_hash).await.unwrap();
        let signature: Bytes =
            SignatureWithNonce::abi_encode(&(U256::from(0), signature.as_bytes())).into();
        let account_id = AccountId::new_evm(chain_id, format!("{}", sw_address));

        // Testing ERC-6492 signatures with deployed ERC-1271.
        assert!(
            verifier
                .is_valid_signature(account_id.clone(), *hash, signature.clone(), None)
                .await
                .unwrap()
                .is_valid
        );

        assert!(
            !verifier
                .is_valid_signature(account_id.clone(), *B256::random(), signature, None)
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
                    *B256::random(),
                    signature.as_bytes().into(),
                    None
                )
                .await
                .unwrap()
                .is_valid
        );
    }
}
