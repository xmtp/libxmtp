//! Interaction with [ERC-1271](https://eips.ethereum.org/EIPS/eip-1271) smart contracts.
use crate::associations::AccountId;
use crate::scw_verifier::{BlockStamp, ChainBlocks, ScwInput, SmartContractSignatureVerifier};
use alloy::eips::BlockNumberOrTag;
use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, BlockNumber, Bytes, FixedBytes};
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::{sol, sol_types::SolConstructor};
use hex::FromHexError;
use std::sync::Arc;

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

/// The code that validates `signature` off chain: the universal signature
/// validator followed by its constructor arguments. For every signature
/// [`ScwInput::checked`] accepts, it fits EIP-3860's initcode limit, and for
/// a signature one byte longer it does not.
fn initcode(
    account_address: &str,
    hash: [u8; 32],
    signature: Bytes,
) -> Result<Vec<u8>, VerifierError> {
    let code = hex::decode(VALIDATE_SIG_OFFCHAIN_BYTECODE.trim())?;
    let account_address: Address = account_address
        .parse()
        .map_err(|_| FromHexError::InvalidStringLength)?;
    let call =
        VerifySig::constructorCall::new((account_address, FixedBytes::<32>::new(hash), signature));
    Ok([code, call.abi_encode()].concat())
}

#[xmtp_common::async_trait]
impl SmartContractSignatureVerifier for RpcSmartContractWalletVerifier {
    /// Checks the input before it builds any code or calls the provider.
    async fn is_valid_signature(
        &self,
        signer: AccountId,
        hash: [u8; 32],
        signature: Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        let input = ScwInput::checked(signer, hash, signature, block_number)?;
        let data = initcode(
            input.account_id().get_account_address(),
            input.hash(),
            input.signature().clone(),
        )?;
        let block_number = match input.block_number() {
            Some(bn) => bn,
            None => self.provider.get_block_number().await?,
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

impl RpcSmartContractWalletVerifier {
    /// The block at `number`, or a retryable error when the chain has none.
    async fn block(&self, number: BlockNumberOrTag) -> Result<BlockStamp, VerifierError> {
        let block =
            self.provider
                .get_block_by_number(number)
                .await?
                .ok_or(VerifierError::MissingBlock(
                    number.as_number().unwrap_or_default(),
                ))?;
        Ok(BlockStamp {
            number: block.header.number,
            timestamp: block.header.timestamp,
        })
    }
}

/// One provider serves one chain, so the chain id is already resolved.
#[xmtp_common::async_trait]
impl ChainBlocks for RpcSmartContractWalletVerifier {
    async fn head(&self, _: &str) -> Result<BlockStamp, VerifierError> {
        self.block(BlockNumberOrTag::Latest).await
    }

    async fn timestamp(&self, _: &str, number: BlockNumber) -> Result<u64, VerifierError> {
        Ok(self.block(number.into()).await?.timestamp)
    }
}

// Anvil does not work with WASM
// because its a wrapper over the system-binary
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) mod tests {
    #![allow(clippy::unwrap_used)]
    use crate::utils::test::{
        EthereumProvider, SignatureWithNonce, SmartWalletContext, docker_provider,
        docker_smart_wallet,
    };

    use super::*;
    use crate::associations::AccountIdError;
    use crate::utils::test::CoinbaseSmartWalletFactory;
    use alloy::dyn_abi::SolType;
    use alloy::primitives::{B256, U256};
    use alloy::providers::ext::AnvilApi;
    use alloy::signers::Signer;
    use alloy::sol_types::{SolCall, SolValue};
    use alloy::transports::mock::Asserter;
    use std::time::Duration;
    use xmtp_common::RetryableError;
    use xmtp_configuration::MAX_SCW_SIGNATURE_BYTES;

    /// EIP-3860's limit on initcode, which no chain executes past.
    const EIP3860_MAX_INITCODE_BYTES: usize = 49_152;

    /// A verifier whose provider fails its first call, so a test can tell
    /// whether a check reached the provider without any chain.
    fn mocked_verifier() -> (RpcSmartContractWalletVerifier, Asserter) {
        let asserter = Asserter::new();
        asserter.push_failure_msg("the provider was called");
        let provider = ProviderBuilder::new().connect_mocked_client(asserter.clone());
        (
            RpcSmartContractWalletVerifier::new_from_provider(provider),
            asserter,
        )
    }

    /// Universal signature validation reports any signature valid for the
    /// zero address, so a zero account is refused for good before the
    /// verifier builds code or calls the provider. The same input for a
    /// non-zero account reaches the provider.
    // verifies: IDENT-093
    #[xmtp_common::test(unwrap_try = true)]
    async fn zero_address_scw_signature_is_rejected_before_chain_access() {
        let (verifier, asserter) = mocked_verifier();
        let mut signature = vec![0; 65];
        signature[64] = 27;
        let check = |address: String| {
            verifier.is_valid_signature(
                AccountId::new_evm(1, address),
                [0; 32],
                signature.clone().into(),
                Some(1),
            )
        };

        let Err(error) = check(format!("0x{}", "0".repeat(40))).await else {
            panic!("a zero account must not verify");
        };
        assert!(
            matches!(
                error,
                VerifierError::InvalidAccountId(AccountIdError::InvalidAddress)
            ),
            "{error}"
        );
        assert!(!error.is_retryable());
        assert_eq!(asserter.read_q().len(), 1, "the provider was called");

        let control = check(format!("0x{}1", "0".repeat(39))).await;
        assert!(matches!(control, Err(VerifierError::Provider(_))));
        assert!(asserter.read_q().is_empty(), "the provider was not called");
    }

    /// A chain RPC URL can carry the operator's key, so a provider failure
    /// keeps its retryable error but never prints the URL.
    #[xmtp_common::test(unwrap_try = true)]
    async fn provider_errors_omit_the_rpc_url() {
        let sentinel = "private-url-sentinel";
        let verifier =
            RpcSmartContractWalletVerifier::new(format!("http://127.0.0.1:1/v2/{sentinel}"))?;
        let mut signature = vec![0; 65];
        signature[64] = 27;
        for block_number in [None, Some(1)] {
            let Err(error) = verifier
                .is_valid_signature(
                    AccountId::new_evm(1, format!("0x{}", "1".repeat(40))),
                    [0; 32],
                    signature.clone().into(),
                    block_number,
                )
                .await
            else {
                panic!("an unreachable provider must not verify");
            };
            assert!(matches!(error, VerifierError::Provider(_)), "{error}");
            assert!(error.is_retryable());
            let printed = format!("{error} {error:?}");
            assert!(!printed.contains(sentinel), "{block_number:?}: {printed}");
        }
        let error = verifier.head("eip155:1").await.unwrap_err();
        let printed = format!("{error} {error:?}");
        assert!(!printed.contains(sentinel), "head: {printed}");
    }

    /// The cap is the verifier's ceiling: the longest signature it admits
    /// builds initcode a chain still executes, and one byte more would build
    /// initcode no chain executes. A signature over the cap is refused for
    /// good before the verifier builds code from it.
    // verifies: IDENT-093
    #[xmtp_common::test(unwrap_try = true)]
    async fn oversized_scw_signature_is_rejected_before_provider() {
        let address = format!("0x{}", "1".repeat(40));
        let initcode_bytes = |length| {
            initcode(&address, [0; 32], vec![0; length].into()).map(|initcode| initcode.len())
        };
        let at_cap = initcode_bytes(MAX_SCW_SIGNATURE_BYTES)?;
        assert!(at_cap <= EIP3860_MAX_INITCODE_BYTES, "{at_cap} bytes");
        let over_cap = initcode_bytes(MAX_SCW_SIGNATURE_BYTES + 1)?;
        assert!(over_cap > EIP3860_MAX_INITCODE_BYTES, "{over_cap} bytes");

        let (verifier, asserter) = mocked_verifier();
        let account = AccountId::new_evm(1, address.clone());
        let length = MAX_SCW_SIGNATURE_BYTES + 1;
        let Err(error) = verifier
            .is_valid_signature(account.clone(), [0; 32], vec![0; length].into(), Some(1))
            .await
        else {
            panic!("a {length}-byte signature must not verify");
        };
        assert!(
            matches!(error, VerifierError::SignatureTooLong(got) if got == length),
            "{error}"
        );
        assert!(!error.is_retryable());
        assert_eq!(asserter.read_q().len(), 1, "the provider was called");

        let reached = verifier
            .is_valid_signature(
                account,
                [0; 32],
                vec![0; MAX_SCW_SIGNATURE_BYTES].into(),
                Some(1),
            )
            .await;
        assert!(matches!(reached, Err(VerifierError::Provider(_))));
        assert!(asserter.read_q().is_empty(), "the provider was not called");
    }

    alloy::sol! {
        struct SignatureWrapper { uint256 ownerIndex; bytes signatureData; }
        struct WebAuthnAuth {
            bytes authenticatorData;
            string clientDataJSON;
            uint256 challengeIndex;
            uint256 typeIndex;
            uint256 r;
            uint256 s;
        }
        interface SafeProxyFactory {
            function createProxyWithNonce(address singleton, bytes initializer, uint256 saltNonce);
        }
        interface Safe {
            function setup(
                address[] owners,
                uint256 threshold,
                address to,
                bytes data,
                address fallbackHandler,
                address paymentToken,
                uint256 payment,
                address paymentReceiver
            );
        }
        interface SafeModuleSetup {
            function enableModules(address[] modules);
        }
    }

    /// An ERC-6492 signature for a wallet not yet deployed: the factory, its
    /// deploy call and the wallet's own signature, then the magic suffix.
    fn counterfactual(factory_calldata: Vec<u8>, signature: Vec<u8>) -> Vec<u8> {
        let mut encoded = (
            Address::repeat_byte(0xfa),
            Bytes::from(factory_calldata),
            Bytes::from(signature),
        )
            .abi_encode_params();
        encoded.extend(alloy::primitives::b256!(
            "6492649264926492649264926492649264926492649264926492649264926492"
        ));
        encoded
    }

    /// The cap leaves twice the room of the two largest common counterfactual
    /// wallet signatures, each estimated from its wallet's own encoding: a
    /// Coinbase Smart Wallet with one passkey owner, and a Safe with five
    /// owners, a module setup call and one nested contract signature among
    /// five. A real signature from each wallet's SDK is measured before
    /// release.
    // verifies: IDENT-093
    #[xmtp_common::test(unwrap_try = true)]
    fn common_wallet_erc6492_signatures_fit_the_cap() {
        // WebAuthn assertion fields: 37 bytes of authenticator data, and the
        // client data a browser returns, here with the extra key Chrome adds.
        let authenticator_data = Bytes::from(vec![0xaa; 37]);
        let challenge = "A".repeat(43);
        let client_data_json = format!(
            r#"{{"type":"webauthn.get","challenge":"{challenge}","origin":"https://keys.coinbase.com","crossOrigin":false,"other_keys_can_be_added_here":"do not compare clientDataJSON against a template. See https://goo.gl/yabPex"}}"#
        );

        let passkey_owner = (U256::MAX, U256::MAX).abi_encode_params();
        let coinbase = counterfactual(
            CoinbaseSmartWalletFactory::createAccountCall {
                owners: vec![passkey_owner.into()],
                nonce: U256::ZERO,
            }
            .abi_encode(),
            SignatureWrapper {
                ownerIndex: U256::ZERO,
                signatureData: WebAuthnAuth {
                    authenticatorData: authenticator_data.clone(),
                    clientDataJSON: client_data_json,
                    challengeIndex: U256::from(23),
                    typeIndex: U256::from(1),
                    r: U256::MAX,
                    s: U256::MAX,
                }
                .abi_encode()
                .into(),
            }
            .abi_encode(),
        );

        let owners: Vec<Address> = (1..=5).map(Address::repeat_byte).collect();
        let initializer = Safe::setupCall {
            owners,
            threshold: U256::from(5),
            to: Address::repeat_byte(0x10),
            data: SafeModuleSetup::enableModulesCall {
                modules: vec![Address::repeat_byte(0x11)],
            }
            .abi_encode()
            .into(),
            fallbackHandler: Address::repeat_byte(0x12),
            paymentToken: Address::ZERO,
            payment: U256::ZERO,
            paymentReceiver: Address::ZERO,
        }
        .abi_encode();
        // Four owner ECDSA signatures, then one contract signature: its
        // static part points at a passkey signer's signature after it.
        let nested = (
            authenticator_data,
            Bytes::from(r#""origin":"https://app.safe.global","crossOrigin":false"#),
            [U256::MAX; 2],
        )
            .abi_encode_params();
        let mut owner_signatures = vec![0x1b; 4 * 65];
        owner_signatures.extend(Address::repeat_byte(0x05).into_word());
        owner_signatures.extend(U256::from(5 * 65).to_be_bytes::<32>());
        owner_signatures.push(0);
        owner_signatures.extend(U256::from(nested.len()).to_be_bytes::<32>());
        owner_signatures.extend(nested);
        let safe = counterfactual(
            SafeProxyFactory::createProxyWithNonceCall {
                singleton: Address::repeat_byte(0x13),
                initializer: initializer.into(),
                saltNonce: U256::ZERO,
            }
            .abi_encode(),
            owner_signatures,
        );

        for (wallet, signature) in [("coinbase", coinbase), ("safe", safe)] {
            assert!(
                2 * signature.len() <= MAX_SCW_SIGNATURE_BYTES,
                "{wallet}: {} bytes",
                signature.len()
            );
        }
    }

    /// Admission freshness compares a signature's block with the head, so the
    /// verifier must report the chain's own head and past block timestamps,
    /// and a block the chain has not produced must be a retryable error rather
    /// than a verdict on the signature.
    #[rstest::rstest]
    #[xmtp_common::timeout(Duration::from_secs(30))]
    #[tokio::test]
    async fn chain_blocks_report_the_head_and_past_timestamps(
        #[future] docker_provider: EthereumProvider,
    ) {
        let provider = docker_provider.await.provider;
        provider.anvil_mine(Some(2), Some(100)).await.unwrap();
        let verifier = RpcSmartContractWalletVerifier::new_from_provider(provider.clone());
        let head = verifier.head("eip155:31337").await.unwrap();
        assert!(head.number <= provider.get_block_number().await.unwrap());
        let block = provider
            .get_block_by_number(head.number.into())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(head.timestamp, block.header.timestamp);
        let earlier = verifier
            .timestamp("eip155:31337", head.number - 1)
            .await
            .unwrap();
        assert!(earlier <= head.timestamp);
        let missing = verifier
            .timestamp("eip155:31337", head.number + 1_000_000)
            .await
            .unwrap_err();
        assert!(matches!(missing, VerifierError::MissingBlock(_)));
        assert!(missing.is_retryable());
    }

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
    // verifies: IDENT-093
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
