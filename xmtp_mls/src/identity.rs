use openmls::{
    prelude::{Credential, CredentialType, CredentialWithKey, CryptoConfig},
    prelude_test::KeyPackage,
    versions::ProtocolVersion,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::{
    types::{Ciphersuite, CryptoError},
    OpenMlsProvider,
};
use thiserror::Error;
use xmtp_cryptography::signature::SignatureError;

use crate::{
    association::AssociationError, storage::StorageError,
    xmtp_openmls_provider::XmtpOpenMlsProvider,
};

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("generating new identity")]
    BadGeneration(#[from] SignatureError),
    #[error("bad association")]
    BadAssocation(#[from] AssociationError),
    #[error("generating key-pairs")]
    KeyGenerationError(#[from] CryptoError),
    #[error("storage error")]
    StorageError(#[from] StorageError),
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Identity {
    pub(crate) credential_with_key: CredentialWithKey,
    pub(crate) signer: SignatureKeyPair,
}

impl Identity {
    pub(crate) fn new(
        ciphersuite: Ciphersuite,
        provider: &XmtpOpenMlsProvider,
        id: &[u8],
    ) -> Result<Self, IdentityError> {
        let credential = Credential::new(id.to_vec(), CredentialType::Basic).unwrap();
        let signature_keys = SignatureKeyPair::new(ciphersuite.signature_algorithm())?;
        let credential_with_key = CredentialWithKey {
            credential,
            signature_key: signature_keys.to_public_vec().into(),
        };
        signature_keys.store(provider.key_store())?;

        // TODO: Make OpenMLS not delete this once used
        let _last_resort_key_package = KeyPackage::builder()
            .build(
                CryptoConfig {
                    ciphersuite,
                    version: ProtocolVersion::default(),
                },
                provider,
                &signature_keys,
                credential_with_key.clone(),
            )
            .unwrap();

        // TODO: upload

        Ok(Self {
            credential_with_key,
            signer: signature_keys,
        })
    }
}
