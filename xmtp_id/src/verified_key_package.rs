/// Key Package Verification (Copied from `xmtp_mls/src/verified_key_package.rs`)
use openmls::{
    credentials::BasicCredential,
    prelude::{
        tls_codec::{Deserialize, Error as TlsSerializationError},
        BasicCredentialError, KeyPackage, KeyPackageIn, KeyPackageVerifyError,
    },
};
use openmls_rust_crypto::RustCrypto;
use thiserror::Error;

use crate::Identity;
use xmtp_mls::{configuration::MLS_PROTOCOL_VERSION, types::Address};

#[derive(Debug, Error)]
pub enum KeyPackageVerificationError {
    #[error("serialization error: {0}")]
    Serialization(#[from] TlsSerializationError),
    #[error("mls validation: {0}")]
    MlsValidation(#[from] KeyPackageVerifyError),
    #[error("identity: {0}")]
    Identity(#[from] crate::IdentityError),
    #[error("invalid application id")]
    InvalidApplicationId,
    #[error("application id ({0}) does not match the credential address ({1}).")]
    ApplicationIdCredentialMismatch(String, String),
    #[error("invalid lifetime")]
    InvalidLifetime,
    #[error("generic: {0}")]
    Generic(String),
    #[error("wrong credential type")]
    WrongCredentialType(#[from] BasicCredentialError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedKeyPackage {
    pub inner: KeyPackage,
    pub account_address: String,
}

impl VerifiedKeyPackage {
    pub fn new(inner: KeyPackage, account_address: String) -> Self {
        Self {
            inner,
            account_address,
        }
    }

    // Validates starting with a KeyPackage (which is already validated by OpenMLS)
    pub async fn from_key_package(kp: KeyPackage) -> Result<Self, KeyPackageVerificationError> {
        let leaf_node = kp.leaf_node();

        let basic_credential = BasicCredential::try_from(leaf_node.credential())?;
        let pub_key_bytes = leaf_node.signature_key().as_slice();
        let account_address =
            identity_to_account_address(basic_credential.identity(), pub_key_bytes).await?;
        let application_id = extract_application_id(&kp)?;
        if !account_address.eq(&application_id) {
            return Err(
                KeyPackageVerificationError::ApplicationIdCredentialMismatch(
                    application_id,
                    account_address,
                ),
            );
        }
        if !kp.life_time().is_valid() {
            return Err(KeyPackageVerificationError::InvalidLifetime);
        }

        Ok(Self::new(kp, account_address))
    }

    // Validates starting with a KeyPackageIn as bytes (which is not validated by OpenMLS)
    pub async fn from_bytes(
        crypto_provider: &RustCrypto,
        data: &[u8],
    ) -> Result<VerifiedKeyPackage, KeyPackageVerificationError> {
        let kp_in: KeyPackageIn = KeyPackageIn::tls_deserialize_exact(data)?;
        let kp = kp_in.validate(crypto_provider, MLS_PROTOCOL_VERSION)?;

        Self::from_key_package(kp).await
    }

    pub fn installation_id(&self) -> Vec<u8> {
        self.inner.leaf_node().signature_key().as_slice().to_vec()
    }

    pub fn hpke_init_key(&self) -> Vec<u8> {
        self.inner.hpke_init_key().as_slice().to_vec()
    }
}

async fn identity_to_account_address(
    credential_bytes: &[u8],
    installation_key_bytes: &[u8],
) -> Result<String, KeyPackageVerificationError> {
    Ok(Identity::get_validated_account_address(credential_bytes, installation_key_bytes).await?)
}

fn extract_application_id(kp: &KeyPackage) -> Result<Address, KeyPackageVerificationError> {
    let application_id_bytes = kp
        .leaf_node()
        .extensions()
        .application_id()
        .ok_or_else(|| KeyPackageVerificationError::InvalidApplicationId)?
        .as_slice()
        .to_vec();

    String::from_utf8(application_id_bytes)
        .map_err(|_| KeyPackageVerificationError::InvalidApplicationId)
}

#[cfg(test)]
mod tests {
    use openmls::{
        credentials::CredentialWithKey,
        extensions::{
            ApplicationIdExtension, Extension, ExtensionType, Extensions, LastResortExtension,
        },
        group::config::CryptoConfig,
        prelude::Capabilities,
        prelude_test::KeyPackage,
        versions::ProtocolVersion,
    };
    use xmtp_cryptography::utils::generate_local_wallet;

    use xmtp_mls::{
        builder::ClientBuilder,
        configuration::CIPHERSUITE,
        verified_key_package::{KeyPackageVerificationError, VerifiedKeyPackage},
    };

    #[tokio::test]
    async fn test_invalid_application_id() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let conn = client.store.conn().unwrap();
        let provider = client.mls_provider(&conn);

        // Build a key package
        let last_resort = Extension::LastResort(LastResortExtension::default());
        // Make sure the application id doesn't match the account address
        let invalid_application_id = "invalid application id".as_bytes();
        let application_id =
            Extension::ApplicationId(ApplicationIdExtension::new(invalid_application_id));
        let leaf_node_extensions = Extensions::single(application_id);
        let capabilities = Capabilities::new(
            None,
            Some(&[CIPHERSUITE]),
            Some(&[ExtensionType::LastResort, ExtensionType::ApplicationId]),
            None,
            None,
        );
        // TODO: Set expiration
        let kp = KeyPackage::builder()
            .leaf_node_capabilities(capabilities)
            .key_package_extensions(Extensions::single(last_resort))
            .leaf_node_extensions(leaf_node_extensions)
            .build(
                CryptoConfig {
                    ciphersuite: CIPHERSUITE,
                    version: ProtocolVersion::default(),
                },
                &provider,
                &client.identity.installation_keys,
                CredentialWithKey {
                    credential: client.identity.credential().unwrap(),
                    signature_key: client.identity.installation_keys.to_public_vec().into(),
                },
            )
            .unwrap();

        let verified_kp_result = VerifiedKeyPackage::from_key_package(kp);
        assert!(verified_kp_result.is_err());
        assert_eq!(
            KeyPackageVerificationError::ApplicationIdCredentialMismatch(
                String::from_utf8(invalid_application_id.to_vec()).unwrap(),
                client.account_address()
            )
            .to_string(),
            verified_kp_result.err().unwrap().to_string()
        );
    }
}
