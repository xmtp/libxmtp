pub mod associations;
pub mod constants;
pub mod erc1271_verifier;
pub mod utils;
use ethers::{
    middleware::Middleware,
    providers::{Http, Provider},
    signers::{LocalWallet, Signer},
    types::Address,
};
use futures::executor;
use openmls_traits::types::CryptoError;
use thiserror::Error;
use xmtp_cryptography::signature::{h160addr_to_string, RecoverableSignature, SignatureError};

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("generating key-pairs: {0}")]
    KeyGenerationError(#[from] CryptoError),
    #[error("uninitialized identity")]
    UninitializedIdentity,
    #[error("protobuf deserialization: {0}")]
    Deserialization(#[from] prost::DecodeError),
    #[error(transparent)]
    ProviderError(#[from] ethers::providers::ProviderError),
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),
}

/// The global InboxID Type.
pub type InboxId = String;

// Check if the given address is a smart contract by checking if there is code at the given address.
pub async fn is_smart_contract(
    address: Address,
    url: String,
    block: Option<u64>,
) -> Result<bool, IdentityError> {
    let provider: Provider<Http> = Provider::<Http>::try_from(url)?;
    let code = provider.get_code(address, block.map(Into::into)).await?;
    Ok(!code.is_empty())
}

pub trait InboxOwner {
    /// Get address of the wallet.
    fn get_address(&self) -> String;
    /// Sign text with the wallet.
    fn sign(&self, text: &str) -> Result<RecoverableSignature, SignatureError>;
}

impl InboxOwner for LocalWallet {
    fn get_address(&self) -> String {
        h160addr_to_string(self.address())
    }

    fn sign(&self, text: &str) -> Result<RecoverableSignature, SignatureError> {
        Ok(executor::block_on(self.sign_message(text))?.to_vec().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::{
        contract::abigen,
        middleware::SignerMiddleware,
        providers::{Http, Provider},
        utils::Anvil,
    };
    use std::sync::Arc;

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
    async fn test_is_smart_contract() {
        let anvil = Anvil::new().args(vec!["--base-fee", "100"]).spawn();
        let deployer: LocalWallet = anvil.keys()[1].clone().into();
        let provider = Provider::<Http>::try_from(anvil.endpoint()).unwrap();
        let client = Arc::new(SignerMiddleware::new(
            provider.clone(),
            deployer.clone().with_chain_id(anvil.chain_id()),
        ));

        // deploy a coinbase smart wallet as the implementation for factory
        let implementation = CoinbaseSmartWallet::deploy(client.clone(), ())
            .unwrap()
            .gas_price(100)
            .send()
            .await
            .unwrap();

        assert!(
            !is_smart_contract(deployer.address(), anvil.endpoint(), None)
                .await
                .unwrap()
        );
        assert!(
            is_smart_contract(implementation.address(), anvil.endpoint(), None)
                .await
                .unwrap()
        );
    }
}
