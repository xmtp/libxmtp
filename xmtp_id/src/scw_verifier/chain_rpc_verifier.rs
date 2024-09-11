//! Interaction with [ERC-1271](https://eips.ethereum.org/EIPS/eip-1271) smart contracts.
use crate::scw_verifier::SmartContractSignatureVerifier;
use async_trait::async_trait;
use ethers::abi::{Constructor, Param, ParamType, Token};
use ethers::contract::abigen;
use ethers::providers::{Http, Middleware, Provider};
use ethers::types::transaction::eip2718::TypedTransaction;
use ethers::types::{Address, BlockId, BlockNumber, Bytes, TransactionRequest};
use hex::{FromHex, FromHexError};
use std::sync::Arc;

use crate::associations::AccountId;

use super::VerifierError;

// https://github.com/AmbireTech/signature-validator/blob/7706bda/index.ts#L13
// Contract from AmbireTech that is also used by Viem.
// Note that this is not a complete ERC-6492 implementation as it lacks Prepare/Side-effect logic compared to official reference implementation, so it might evolve in the future.
// For now it's accepted as [Coinbase Smart Wallet doc](https://github.com/AmbireTech/signature-validator/blob/7706bda/index.ts#L13) uses it for offchain verification.
const VALIDATE_SIG_OFFCHAIN_BYTECODE: &str = "60806040523480156200001157600080fd5b50604051620007003803806200070083398101604081905262000034916200056f565b6000620000438484846200004f565b9050806000526001601ff35b600080846001600160a01b0316803b806020016040519081016040528181526000908060200190933c90507f6492649264926492649264926492649264926492649264926492649264926492620000a68462000451565b036200021f57600060608085806020019051810190620000c79190620005ce565b8651929550909350915060000362000192576000836001600160a01b031683604051620000f5919062000643565b6000604051808303816000865af19150503d806000811462000134576040519150601f19603f3d011682016040523d82523d6000602084013e62000139565b606091505b5050905080620001905760405162461bcd60e51b815260206004820152601e60248201527f5369676e617475726556616c696461746f723a206465706c6f796d656e74000060448201526064015b60405180910390fd5b505b604051630b135d3f60e11b808252906001600160a01b038a1690631626ba7e90620001c4908b90869060040162000661565b602060405180830381865afa158015620001e2573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906200020891906200069d565b6001600160e01b031916149450505050506200044a565b805115620002b157604051630b135d3f60e11b808252906001600160a01b03871690631626ba7e9062000259908890889060040162000661565b602060405180830381865afa15801562000277573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906200029d91906200069d565b6001600160e01b031916149150506200044a565b8251604114620003195760405162461bcd60e51b815260206004820152603a6024820152600080516020620006e083398151915260448201527f3a20696e76616c6964207369676e6174757265206c656e677468000000000000606482015260840162000187565b620003236200046b565b506020830151604080850151855186939260009185919081106200034b576200034b620006c9565b016020015160f81c9050601b81148015906200036b57508060ff16601c14155b15620003cf5760405162461bcd60e51b815260206004820152603b6024820152600080516020620006e083398151915260448201527f3a20696e76616c6964207369676e617475726520762076616c75650000000000606482015260840162000187565b6040805160008152602081018083528a905260ff83169181019190915260608101849052608081018390526001600160a01b038a169060019060a0016020604051602081039080840390855afa1580156200042e573d6000803e3d6000fd5b505050602060405103516001600160a01b031614955050505050505b9392505050565b60006020825110156200046357600080fd5b508051015190565b60405180606001604052806003906020820280368337509192915050565b6001600160a01b03811681146200049f57600080fd5b50565b634e487b7160e01b600052604160045260246000fd5b60005b83811015620004d5578181015183820152602001620004bb565b50506000910152565b600082601f830112620004f057600080fd5b81516001600160401b03808211156200050d576200050d620004a2565b604051601f8301601f19908116603f01168101908282118183101715620005385762000538620004a2565b816040528381528660208588010111156200055257600080fd5b62000565846020830160208901620004b8565b9695505050505050565b6000806000606084860312156200058557600080fd5b8351620005928162000489565b6020850151604086015191945092506001600160401b03811115620005b657600080fd5b620005c486828701620004de565b9150509250925092565b600080600060608486031215620005e457600080fd5b8351620005f18162000489565b60208501519093506001600160401b03808211156200060f57600080fd5b6200061d87838801620004de565b935060408601519150808211156200063457600080fd5b50620005c486828701620004de565b6000825162000657818460208701620004b8565b9190910192915050565b828152604060208201526000825180604084015262000688816060850160208701620004b8565b601f01601f1916919091016060019392505050565b600060208284031215620006b057600080fd5b81516001600160e01b0319811681146200044a57600080fd5b634e487b7160e01b600052603260045260246000fdfe5369676e617475726556616c696461746f72237265636f7665725369676e6572";

abigen!(
    ERC1271,
    r#"[
        function isValidSignature(bytes32 hash, bytes calldata signature) public view virtual returns (bytes4 result)
    ]"#,
    derives(serde::Serialize, serde::Deserialize)
);

#[derive(Debug)]
pub struct RpcSmartContractWalletVerifier {
    pub provider: Arc<Provider<Http>>,
}

impl RpcSmartContractWalletVerifier {
    pub fn new(url: String) -> Self {
        let provider = Arc::new(Provider::<Http>::try_from(url).unwrap());
        Self { provider }
    }
}

#[async_trait]
impl SmartContractSignatureVerifier for RpcSmartContractWalletVerifier {
    /// Verifies an ERC-6492<https://eips.ethereum.org/EIPS/eip-6492> signature.
    ///
    /// # Arguments
    ///
    /// * `block_number` - Block number to verify the signature at.
    /// * `signer` - can be the smart wallet address or EOA address.
    /// * `hash` - Message digest for the signature.
    /// * `signature` - Could be encoded smart wallet signature or raw ECDSA signature.
    async fn is_valid_signature(
        &self,
        signer: AccountId,
        hash: [u8; 32],
        signature: &Bytes,
        block_number: Option<BlockNumber>,
    ) -> Result<bool, VerifierError> {
        let code = hex::decode(VALIDATE_SIG_OFFCHAIN_BYTECODE).unwrap();
        let account_address: Address = signer
            .account_address
            .parse()
            .map_err(|_| FromHexError::InvalidStringLength)?;
        // ABI of the ValidateSigOffchain constructor
        // constructor (address _signer, bytes32 _hash, bytes memory _signature)
        let inputs: Vec<Param> = vec![
            Param {
                name: "_signer".to_string(),
                kind: ParamType::Address,
                internal_type: Some("_signer".into()),
            },
            Param {
                name: "_hash".to_string(),
                kind: ParamType::FixedBytes(32),
                internal_type: Some("_hash".to_string()),
            },
            Param {
                name: "_signature".to_string(),
                kind: ParamType::Bytes,
                internal_type: Some("_signature".to_string()),
            },
        ];
        let constructor = Constructor { inputs };
        let tokens = &[
            Token::Address(account_address),
            Token::FixedBytes(hash.to_vec()),
            Token::Bytes(signature.to_vec()),
        ];
        let data = constructor.encode_input(code, tokens)?;
        let tx: TypedTransaction = TransactionRequest::new().data(data).into();
        let res = self
            .provider
            .call(&tx, block_number.map(BlockId::Number))
            .await?;
        Ok(res == Bytes::from_hex("0x01").unwrap())
    }
}

#[cfg(test)]
pub mod tests {
    use crate::is_smart_contract;

    use super::*;
    use ethers::{
        abi::{self, Token},
        core::utils::Anvil,
        middleware::{MiddlewareBuilder, SignerMiddleware},
        providers::Middleware,
        signers::{LocalWallet, Signer as _},
        types::{H256, U256},
        utils::{hash_message, AnvilInstance},
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

                let contract_call = factory.create_account(owners_addresses.clone(), nonce);
                let pending_tx = contract_call.send().await.unwrap();
                pending_tx.await.unwrap();

                // Generate signatures from owners and verify them.
                let smart_wallet = CoinbaseSmartWallet::new(
                    smart_wallet_address,
                    Arc::new(client.with_signer(owner0.clone().with_chain_id(anvil.chain_id()))),
                );
                let hash: [u8; 32] = H256::random().into();
                let replay_safe_hash = smart_wallet.replay_safe_hash(hash).call().await.unwrap();
                let verifier = RpcSmartContractWalletVerifier::new(anvil.endpoint());

                // verify owner0 is a valid owner
                let sig0 = owner0.sign_hash(replay_safe_hash.into()).unwrap();
                let account_id =
                    AccountId::new_evm(anvil.chain_id(), format!("{:?}", smart_wallet_address));
                let res = verifier
                    .is_valid_signature(
                        account_id.clone(),
                        hash,
                        &abi::encode(&[Token::Tuple(vec![
                            Token::Uint(U256::from(0)),
                            Token::Bytes(sig0.to_vec()),
                        ])])
                        .into(),
                        None,
                    )
                    .await
                    .unwrap();
                assert!(res);
                // verify owner1 is a valid owner
                let sig1 = owner1.sign_hash(replay_safe_hash.into()).unwrap();
                let res = verifier
                    .is_valid_signature(
                        account_id.clone(),
                        hash,
                        &abi::encode(&[Token::Tuple(vec![
                            Token::Uint(U256::from(1)),
                            Token::Bytes(sig1.to_vec()),
                        ])])
                        .into(),
                        None,
                    )
                    .await
                    .unwrap();
                assert!(res);
                // owner0 siganture must not be used to verify owner1
                let res = verifier
                    .is_valid_signature(
                        account_id.clone(),
                        hash,
                        &abi::encode(&[Token::Tuple(vec![
                            Token::Uint(U256::from(1)),
                            Token::Bytes(sig0.to_vec()),
                        ])])
                        .into(),
                        None,
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
                        account_id.clone(),
                        hash,
                        &abi::encode(&[Token::Tuple(vec![
                            Token::Uint(U256::from(1)),
                            Token::Bytes(sig1.to_vec()),
                        ])])
                        .into(),
                        None,
                    )
                    .await;
                assert!(res.is_err()); // when verify a non-existing owner, it errors

                // time travel to the pre-removel block number and verify owner1 WAS a valid owner
                let res = verifier
                    .is_valid_signature(
                        account_id.clone(),
                        hash,
                        &abi::encode(&[Token::Tuple(vec![
                            Token::Uint(U256::from(1)),
                            Token::Bytes(sig1.to_vec()),
                        ])])
                        .into(),
                        Some(BlockNumber::Number(block_number)),
                    )
                    .await
                    .unwrap();
                assert!(res);
            }
        })
        .await;
    }

    // Testing ERC-6492 with deployed / undeployed coinbase smart wallet(ERC-1271) contracts, and EOA.
    #[tokio::test]
    async fn test_is_valid_signature() {
        with_smart_contracts(|anvil, _provider, client, smart_contracts| async move {
            // Create owner EOA wallet and then create smart contract wallet account from the factory.
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

            assert!(
                is_smart_contract(smart_wallet_address, anvil.endpoint(), None)
                    .await
                    .unwrap()
            );

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

            let verifier = RpcSmartContractWalletVerifier::new(anvil.endpoint());

            let account_id =
                AccountId::new_evm(anvil.chain_id(), format!("{:?}", smart_wallet_address));

            // Testing ERC-6492 signatures with deployed ERC-1271.
            assert!(verifier
                .is_valid_signature(account_id.clone(), hash, &signature, None,)
                .await
                .unwrap());

            assert!(!verifier
                .is_valid_signature(account_id.clone(), H256::random().into(), &signature, None,)
                .await
                .unwrap());

            // Testing if EOA wallet signature is valid on ERC-6492
            let signature = owner.sign_hash(hash.into()).unwrap();
            let owner_account_id =
                AccountId::new_evm(anvil.chain_id(), format!("{:?}", owner.address()));
            assert!(verifier
                .is_valid_signature(
                    owner_account_id.clone(),
                    hash,
                    &signature.to_vec().into(),
                    None,
                )
                .await
                .unwrap());

            assert!(!verifier
                .is_valid_signature(
                    owner_account_id,
                    H256::random().into(),
                    &signature.to_vec().into(),
                    None,
                )
                .await
                .unwrap());
        })
        .await;
    }

    // This aims to verify a wrapped ERC-6492 signature that has magic bytes against an ERC-1271 ambire wallet deployed on Polygon.
    // This doesn't cover the undeployed case.
    #[ignore] // This test is temporarily being ignored as it relies on an external service
    #[tokio::test]
    async fn test_erc6492_ambire_wallet() {
        let signer = "0x4836a472ab1dd406ecb8d0f933a985541ee3921f".to_string();

        let hash = hex::decode("787177").unwrap();
        let hash = hash_message(hash);
        let signature = Bytes::from_hex("0x000000000000000000000000bf07a0df119ca234634588fbdb5625594e2a5bca00000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000420000000000000000000000000000000000000000000000000000000000000038449c81579000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000010000000000000000000000004836a472ab1dd406ecb8d0f933a985541ee3921f0000000000000000000000000000000000000000000000000000000000000120000000000000000000000000000000000000000000000000000000000000007a7f00000000000000000000000000000000000000000000000000000000000000017f7f0f292b79d9ce101861526459da50f62368077ae24affe97b792bf4bdd2e171553d602d80604d3d3981f3363d3d373d3d3d363d732a2b85eb1054d6f0c6c2e37da05ed3e5fea684ef5af43d82803e903d91602b57fd5bf300000000000000000000000000000000000000000000000000000000000000000000000002246171d1c9000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000001a00000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000004836a472ab1dd406ecb8d0f933a985541ee3921f000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000942f9ce5d9a33a82f88d233aeb3292e6802303480000000000000000000000000000000000000000000000000014c3c6ef1cdc01000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000042f2eaaebf45fc0340eb55f11c52a30e2ca7f48539d0a1f1cdc240482210326494545def903e8ed4441bd5438109abe950f1f79baf032f184728ba2d4161dea32e1b0100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000042c0f8db6019888d87a0afc1299e81ef45d3abce64f63072c8d7a6ef00f5f82c1522958ff110afa98b8c0d23b558376db1d2fbab4944e708f8bf6dc7b977ee07201b000000000000000000000000000000000000000000000000000000000000006492649264926492649264926492649264926492649264926492649264926492").unwrap();

        let verifier = RpcSmartContractWalletVerifier::new("https://polygon-rpc.com".to_string());
        assert!(verifier
            .is_valid_signature(AccountId::new_evm(1, signer), hash.into(), &signature, None)
            .await
            .unwrap());
    }
}
