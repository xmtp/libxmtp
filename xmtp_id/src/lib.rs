pub mod associations;
pub mod credential_verifier;
pub mod erc1271_verifier;

use std::sync::RwLock;

use openmls::prelude::Credential as OpenMlsCredential;
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::types::CryptoError;
use thiserror::Error;
use xmtp_mls::{
    configuration::CIPHERSUITE,
    credential::{AssociationError, Credential, UnsignedGrantMessagingAccessData},
    types::Address,
    utils::time::now_ns,
};

use crate::credential_verifier::{CredentialVerifier, VerificationError, VerificationRequest};

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("bad association: {0}")]
    BadAssocation(#[from] AssociationError),
    #[error("generating key-pairs: {0}")]
    KeyGenerationError(#[from] CryptoError),
    #[error("uninitialized identity")]
    UninitializedIdentity,
    #[error("protobuf deserialization: {0}")]
    Deserialization(#[from] prost::DecodeError),
    #[error("credential verification {0}")]
    VerificationError(#[from] VerificationError),
}

#[async_trait::async_trait]
pub trait WalletIdentity {
    async fn is_smart_wallet(&self, block: Option<u64>) -> Result<bool, IdentityError>;
}

pub struct Identity {
    #[allow(dead_code)]
    pub(crate) account_address: Address,
    #[allow(dead_code)]
    pub(crate) installation_keys: SignatureKeyPair,
    pub(crate) credential: RwLock<Option<OpenMlsCredential>>,
    pub(crate) unsigned_association_data: Option<UnsignedGrantMessagingAccessData>,
}

impl Identity {
    // Creates a credential that is not yet wallet signed. Implementors should sign the payload returned by 'text_to_sign'
    // and call 'register' with the signature.
    #[allow(dead_code)]
    pub(crate) fn create_to_be_signed(account_address: String) -> Result<Self, IdentityError> {
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        let unsigned_association_data = UnsignedGrantMessagingAccessData::new(
            account_address.clone(),
            signature_keys.to_public_vec(),
            now_ns() as u64,
        )?;
        let identity = Self {
            account_address,
            installation_keys: signature_keys,
            credential: RwLock::new(None),
            unsigned_association_data: Some(unsigned_association_data),
        };

        Ok(identity)
    }

    pub fn text_to_sign(&self) -> Option<String> {
        if self.credential().is_ok() {
            return None;
        }
        self.unsigned_association_data
            .clone()
            .map(|data| data.text())
    }

    fn credential(&self) -> Result<OpenMlsCredential, IdentityError> {
        self.credential
            .read()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
            .ok_or(IdentityError::UninitializedIdentity)
    }

    /// Get an account address verified by the
    pub async fn get_validated_account_address(
        credential: &[u8],
        installation_public_key: &[u8],
    ) -> Result<String, IdentityError> {
        let request = VerificationRequest::new(credential, installation_public_key);
        let credential = <Credential as CredentialVerifier>::verify_credential(request).await?;
        Ok(credential.account_address().to_string())
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
    use xmtp_mls::InboxOwner;

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
