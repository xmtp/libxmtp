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

    /// Verifies an ERC-1271(https://eips.ethereum.org/EIPS/eip-1271) signature.
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
mod tests {
    use super::*;
    use ethers::{
        abi::{self, Token},
        providers::Middleware,
        types::{H256, U256},
    };

    use ethers::{
        core::utils::Anvil,
        middleware::SignerMiddleware,
        signers::{LocalWallet, Signer as _},
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

    #[tokio::test]
    async fn test_coinbase_smart_wallet() {
        let anvil = Anvil::new().args(vec!["--base-fee", "100"]).spawn();
        let owner0: LocalWallet = anvil.keys()[1].clone().into();
        let owner1: LocalWallet = anvil.keys()[2].clone().into();
        let owners = vec![
            Bytes::from(H256::from(owner0.address()).0.to_vec()),
            Bytes::from(H256::from(owner1.address()).0.to_vec()),
        ];
        let nonce = U256::from(0); // needed when creating a smart wallet
        let provider = Provider::<Http>::try_from(anvil.endpoint()).unwrap();
        let client = Arc::new(SignerMiddleware::new(
            provider.clone(),
            owner0.clone().with_chain_id(anvil.chain_id()),
        ));
        let verifier = ERC1271Verifier::new(anvil.endpoint());

        // deploy a coinbase smart wallet as the implementation for factory
        let implementation = CoinbaseSmartWallet::deploy(client.clone(), ())
            .unwrap()
            .gas_price(100)
            .send()
            .await
            .unwrap();

        // Deploy the factory
        let factory = CoinbaseSmartWalletFactory::deploy(client.clone(), implementation.address())
            .unwrap()
            .gas_price(100)
            .send()
            .await
            .unwrap();

        let smart_wallet_address = factory.get_address(owners.clone(), nonce).await.unwrap();
        let tx = factory.create_account(owners.clone(), nonce);
        let pending_tx = tx.send().await.unwrap();
        let _ = pending_tx.await.unwrap();

        // Generate signatures from owners and verify them.
        let smart_wallet = CoinbaseSmartWallet::new(smart_wallet_address, client.clone());
        let hash: [u8; 32] = H256::random().into();
        let replay_safe_hash = smart_wallet.replay_safe_hash(hash).call().await.unwrap();
        // owner 0
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
        assert_eq!(res, true);
        // owner1
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
        assert_eq!(res, true);
        // owner0 siganture won't be deemed as signed by owner1
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
        assert_eq!(res, false);

        // get block number before removing
        let block_number = provider.get_block_number().await.unwrap();

        // remove owner1 and check their signature
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

        // use pre-removal block number to verify owner1 signature
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
        assert_eq!(res, true);
    }
}
