#![warn(clippy::unwrap_used)]
pub mod associations;
pub mod constants;
pub mod scw_verifier;
pub mod utils;
use ethers::{
    middleware::Middleware,
    providers::{Http, Provider},
    signers::{LocalWallet, Signer},
    types::Address,
};
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
        let message_hash = ethers::core::utils::hash_message(text);
        Ok(self.sign_hash(message_hash)?.to_vec().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scw_verifier::tests::with_smart_contracts;
    use ethers::contract::abigen;

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
        with_smart_contracts(|anvil, _provider, _client, smart_contracts| async move {
            let deployer: LocalWallet = anvil.keys()[0].clone().into();
            let factory = smart_contracts.coinbase_smart_wallet_factory();
            assert!(
                !is_smart_contract(deployer.address(), anvil.endpoint(), None)
                    .await
                    .unwrap()
            );
            assert!(is_smart_contract(factory.address(), anvil.endpoint(), None)
                .await
                .unwrap());
        })
        .await;
    }
}
