//! Interaction with [ERC-1271](https://eips.ethereum.org/EIPS/eip-1271) smart contracts.
use ethers::contract::abigen;
use ethers::providers::{Http, Provider};
use ethers::types::{Address, BlockNumber, Bytes};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VerifierError {
    #[error("calling smart contract {0}")]
    Contract(#[from] ethers::contract::ContractError<Provider<Http>>),
}

const EIP1271_MAGIC_VALUE: [u8; 4] = [0x16, 0x26, 0xba, 0x7e];

abigen!(
    ERC1271,
    r#"[
        function isValidSignature(bytes32 hash, bytes calldata signature) public view virtual returns (bytes4 result)
    ]"#,
    derives(serde::Serialize, serde::Deserialize)
);

#[derive(Debug)]
pub struct ERC1271Verifier {
    pub provider: Arc<Provider<Http>>,
}

impl ERC1271Verifier {
    pub fn new(url: String) -> Self {
        let provider = Arc::new(Provider::<Http>::try_from(url).unwrap());
        Self { provider }
    }

    /// Verifies an ERC-1271<https://eips.ethereum.org/EIPS/eip-1271> signature.
    ///
    /// # Arguments
    ///
    /// * `contract_address` - Address of the ERC1271 wallet.
    /// * `block_number` - Block number to verify the signature at.
    /// * `hash`, `signature` - Inputs to ERC-1271, used for signer verification.
    pub async fn is_valid_signature(
        &self,
        contract_address: Address,
        block_number: Option<BlockNumber>,
        hash: [u8; 32],
        signature: Bytes,
    ) -> Result<bool, VerifierError> {
        let erc1271 = ERC1271::new(contract_address, self.provider.clone());

        let res: [u8; 4] = erc1271
            .is_valid_signature(hash, signature)
            .block(block_number.unwrap_or_default())
            .call()
            .await?;

        Ok(res == EIP1271_MAGIC_VALUE)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ethers::{
        abi::{self, Token},
        core::utils::Anvil,
        middleware::{MiddlewareBuilder, SignerMiddleware},
        providers::Middleware,
        signers::{LocalWallet, Signer as _},
        types::{H256, U256},
        utils::AnvilInstance,
    };

    abigen!(
        CoinbaseSmartWallet,
        "artifact/CoinbaseSmartWallet.json",
        derives(serde::Serialize, serde::Deserialize)
    );

    abigen!(
        CoinbaseSmartWalletFactory,
        "artifact/CoinbaseSmartWalletFactory.json",
        derives(serde::Serialize, serde::Deserialize)
    );

    // keys for smart contract hashmap
    pub struct SmartContracts {
        coinbase_smart_wallet_factory:
            CoinbaseSmartWalletFactory<SignerMiddleware<Provider<Http>, LocalWallet>>,
    }

    impl SmartContracts {
        fn new(
            coinbase_smart_wallet_factory: CoinbaseSmartWalletFactory<
                SignerMiddleware<Provider<Http>, LocalWallet>,
            >,
        ) -> Self {
            Self {
                coinbase_smart_wallet_factory,
            }
        }

        pub fn coinbase_smart_wallet_factory(
            &self,
        ) -> &CoinbaseSmartWalletFactory<SignerMiddleware<Provider<Http>, LocalWallet>> {
            &self.coinbase_smart_wallet_factory
        }
    }

    /// Test harness that loads a local anvil node with deployed smart contracts.
    pub async fn with_smart_contracts<Func, Fut>(fun: Func)
    where
        Func: FnOnce(
            AnvilInstance,
            Provider<Http>,
            SignerMiddleware<Provider<Http>, LocalWallet>,
            SmartContracts,
        ) -> Fut,
        Fut: futures::Future<Output = ()>,
    {
        let anvil = Anvil::new().args(vec!["--base-fee", "100"]).spawn();
        let contract_deployer: LocalWallet = anvil.keys()[9].clone().into();
        let provider = Provider::<Http>::try_from(anvil.endpoint()).unwrap();
        let client = SignerMiddleware::new(
            provider.clone(),
            contract_deployer.clone().with_chain_id(anvil.chain_id()),
        );
        // 1. coinbase smart wallet
        // deploy implementation for factory
        let implementation = CoinbaseSmartWallet::deploy(Arc::new(client.clone()), ())
            .unwrap()
            .gas_price(100)
            .send()
            .await
            .unwrap();
        // deploy factory
        let factory =
            CoinbaseSmartWalletFactory::deploy(Arc::new(client.clone()), implementation.address())
                .unwrap()
                .gas_price(100)
                .send()
                .await
                .unwrap();

        let smart_contracts = SmartContracts::new(factory);
        fun(anvil, provider.clone(), client.clone(), smart_contracts).await
    }

    #[tokio::test]
    async fn test_coinbase_smart_wallet() {
        with_smart_contracts(|anvil, provider, client, smart_contracts| {
            async move {
                let owner0: LocalWallet = anvil.keys()[0].clone().into();
                let owner1: LocalWallet = anvil.keys()[1].clone().into();
                let owners_addresses = vec![
                    Bytes::from(H256::from(owner0.address()).0.to_vec()),
                    Bytes::from(H256::from(owner1.address()).0.to_vec()),
                ];
                let factory = smart_contracts.coinbase_smart_wallet_factory();
                let nonce = U256::from(0); // needed when creating a smart wallet
                let smart_wallet_address = factory
                    .get_address(owners_addresses.clone(), nonce)
                    .await
                    .unwrap();
                let tx = factory.create_account(owners_addresses.clone(), nonce);
                let pending_tx = tx.send().await.unwrap();
                let _ = pending_tx.await.unwrap();

                // Generate signatures from owners and verify them.
                let smart_wallet = CoinbaseSmartWallet::new(
                    smart_wallet_address,
                    Arc::new(client.with_signer(owner0.clone().with_chain_id(anvil.chain_id()))),
                );
                let hash: [u8; 32] = H256::random().into();
                let replay_safe_hash = smart_wallet.replay_safe_hash(hash).call().await.unwrap();
                let verifier = ERC1271Verifier::new(anvil.endpoint());

                // verify owner0 is a valid owner
                let sig0 = owner0.sign_hash(replay_safe_hash.into()).unwrap();
                let res = verifier
                    .is_valid_signature(
                        smart_wallet_address,
                        None,
                        hash,
                        abi::encode(&[Token::Tuple(vec![
                            Token::Uint(U256::from(0)),
                            Token::Bytes(sig0.to_vec()),
                        ])])
                        .into(),
                    )
                    .await
                    .unwrap();
                assert!(res);
                // verify owner1 is a valid owner
                let sig1 = owner1.sign_hash(replay_safe_hash.into()).unwrap();
                let res = verifier
                    .is_valid_signature(
                        smart_wallet_address,
                        None,
                        hash,
                        abi::encode(&[Token::Tuple(vec![
                            Token::Uint(U256::from(1)),
                            Token::Bytes(sig1.to_vec()),
                        ])])
                        .into(),
                    )
                    .await
                    .unwrap();
                assert!(res);
                // owner0 siganture must not be used to verify owner1
                let res = verifier
                    .is_valid_signature(
                        smart_wallet_address,
                        None,
                        hash,
                        abi::encode(&[Token::Tuple(vec![
                            Token::Uint(U256::from(1)),
                            Token::Bytes(sig0.to_vec()),
                        ])])
                        .into(),
                    )
                    .await
                    .unwrap();
                assert!(!res);

                // Testing time travel
                // get block number before removing the owner.
                let block_number = provider.get_block_number().await.unwrap();

                // remove owner1 and check owner1 is no longer a valid owner
                let tx = smart_wallet.remove_owner_at_index(1.into());
                let pending_tx = tx.send().await.unwrap();
                let _ = pending_tx.await.unwrap();

                let res = verifier
                    .is_valid_signature(
                        smart_wallet_address,
                        None,
                        hash,
                        abi::encode(&[Token::Tuple(vec![
                            Token::Uint(U256::from(1)),
                            Token::Bytes(sig1.to_vec()),
                        ])])
                        .into(),
                    )
                    .await;
                assert!(res.is_err()); // when verify a non-existing owner, it errors

                // time travel to the pre-removel block number and verify owner1 WAS a valid owner
                let res = verifier
                    .is_valid_signature(
                        smart_wallet_address,
                        Some(BlockNumber::Number(block_number)),
                        hash,
                        abi::encode(&[Token::Tuple(vec![
                            Token::Uint(U256::from(1)),
                            Token::Bytes(sig1.to_vec()),
                        ])])
                        .into(),
                    )
                    .await
                    .unwrap();
                assert!(res);
            }
        })
        .await;
    }
}
