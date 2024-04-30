pub mod associations;
pub mod constants;
pub mod erc1271_verifier;
pub mod utils;
use ethers::signers::{LocalWallet, Signer};
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
}

/// The global InboxID Type.
pub type InboxId = String;

#[async_trait::async_trait]
pub trait WalletIdentity {
    async fn is_smart_wallet(&self, block: Option<u64>) -> Result<bool, IdentityError>;
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

/// XMTP Identity according to [XIP-46](https://github.com/xmtp/XIPs/pull/53)
pub struct Identity {
    #[allow(dead_code)]
    id: String,
}

impl Identity {
    /// Generate a new, empty ID for an account address.
    /// A nonce is used to ensure uniqueness of the ID.
    pub fn new(address: String) -> Self {
        // TODO: how to nonce?
        let id = associations::generate_inbox_id(&address, &0);
        Self { id }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::{
        middleware::Middleware,
        providers::{Http, Provider},
        types::Address,
    };
    use std::str::FromStr;
    use xmtp_cryptography::utils::generate_local_wallet;

    struct EthereumWallet {
        provider: Provider<Http>,
        address: String,
    }

    impl EthereumWallet {
        pub fn new(address: String) -> Self {
            let provider = Provider::<Http>::try_from("https://eth.llamarpc.com").unwrap();
            Self { provider, address }
        }
    }

    #[async_trait::async_trait]
    impl WalletIdentity for EthereumWallet {
        async fn is_smart_wallet(&self, block: Option<u64>) -> Result<bool, IdentityError> {
            let address = Address::from_str(&self.address).unwrap();
            let res = self.provider.get_code(address, block.map(Into::into)).await;
            Ok(!res.unwrap().to_vec().is_empty())
        }
    }

    #[tokio::test]
    async fn test_is_smart_wallet() {
        let wallet = generate_local_wallet();
        let eth = EthereumWallet::new(wallet.get_address());
        let scw = EthereumWallet::new("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".into());

        assert!(!eth.is_smart_wallet(None).await.unwrap());
        assert!(scw.is_smart_wallet(None).await.unwrap());
    }
}
