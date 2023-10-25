use openmls::{
    prelude::{
        Credential, CredentialType, CredentialWithKey, CryptoConfig, KeyPackage, KeyPackageNewError,
    },
    versions::ProtocolVersion,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::{
    types::{Ciphersuite, CryptoError},
    OpenMlsProvider,
};
use prost::Message;
use thiserror::Error;
use xmtp_cryptography::signature::SignatureError;

use crate::{
    association::{AssociationError, AssociationText, Eip191Association},
    storage::StorageError,
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    InboxOwner,
};
use xmtp_proto::xmtp::v3::message_contents::Eip191Association as Eip191AssociationProto;

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
    #[error("generating key package")]
    KeyPackageGenerationError(#[from] KeyPackageNewError<StorageError>),
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Identity {
    account_address: String,
    signer: SignatureKeyPair,
    credential: Credential,
}

impl Identity {
    pub(crate) fn new(
        ciphersuite: Ciphersuite,
        provider: &XmtpOpenMlsProvider,
        owner: &impl InboxOwner,
    ) -> Result<Self, IdentityError> {
        let signature_keys = SignatureKeyPair::new(ciphersuite.signature_algorithm())?;
        signature_keys.store(provider.key_store())?;

        let credential = Identity::create_credential(&signature_keys, owner)?;

        // The builder automatically stores it in the key store
        // TODO: Make OpenMLS not delete this once used
        let _last_resort_key_package = KeyPackage::builder().build(
            CryptoConfig {
                ciphersuite,
                version: ProtocolVersion::default(),
            },
            provider,
            &signature_keys,
            CredentialWithKey {
                credential: credential.clone(),
                signature_key: signature_keys.to_public_vec().into(),
            },
        )?;

        // TODO: persist identity
        // TODO: upload credential_with_key and last_resort_key_package

        Ok(Self {
            account_address: owner.get_address(),
            signer: signature_keys,
            credential,
        })
    }

    fn create_credential(
        signature_keys: &SignatureKeyPair,
        owner: &impl InboxOwner,
    ) -> Result<Credential, IdentityError> {
        // Generate association
        let assoc_text = AssociationText::Static {
            blockchain_address: owner.get_address(),
            installation_public_key: signature_keys.to_public_vec(),
        };
        let signature = owner.sign(&assoc_text.text())?;
        let association = Eip191Association::new(signature_keys.public(), assoc_text, signature)?;
        // TODO wrap in a Credential proto to allow flexibility for different association types
        let association_proto: Eip191AssociationProto = association.into();

        // Serialize into credential
        Ok(Credential::new(association_proto.encode_to_vec(), CredentialType::Basic).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{
        configuration::CIPHERSUITE, storage::EncryptedMessageStore,
        xmtp_openmls_provider::XmtpOpenMlsProvider,
    };

    use super::Identity;

    #[test]
    fn does_not_error() {
        Identity::new(
            CIPHERSUITE,
            &XmtpOpenMlsProvider::new(&EncryptedMessageStore::default()),
            &generate_local_wallet(),
        )
        .unwrap();
    }
}
