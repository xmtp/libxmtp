use chrono::Utc;
use openmls::{
    extensions::LastResortExtension,
    prelude::{
        Capabilities, Credential, CredentialType, CredentialWithKey, CryptoConfig, Extension,
        ExtensionType, Extensions, KeyPackage, KeyPackageNewError,
    },
    versions::ProtocolVersion,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::{types::CryptoError, OpenMlsProvider};
use prost::Message;
use thiserror::Error;
use xmtp_cryptography::signature::SignatureError;
use xmtp_proto::xmtp::mls::message_contents::Eip191Association as Eip191AssociationProto;

use crate::{
    association::{AssociationContext, AssociationError, AssociationText, Eip191Association},
    configuration::CIPHERSUITE,
    storage::{identity::StoredIdentity, StorageError},
    types::Address,
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    InboxOwner, Store,
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
    #[error("generating key package")]
    KeyPackageGenerationError(#[from] KeyPackageNewError<StorageError>),
    #[error("deserialization")]
    Deserialization(#[from] prost::DecodeError),
}

#[derive(Debug)]
pub struct Identity {
    pub(crate) account_address: Address,
    pub(crate) installation_keys: SignatureKeyPair,
    pub(crate) credential: Credential,
}

impl Identity {
    pub(crate) fn new(
        provider: &XmtpOpenMlsProvider,
        owner: &impl InboxOwner,
    ) -> Result<Self, IdentityError> {
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        signature_keys.store(provider.key_store())?;

        let credential = Identity::create_credential(&signature_keys, owner)?;

        let identity = Self {
            account_address: owner.get_address(),
            installation_keys: signature_keys,
            credential,
        };

        identity.new_key_package(provider)?;
        StoredIdentity::from(&identity).store(provider.conn())?;

        // TODO: upload credential_with_key and last_resort_key_package

        Ok(identity)
    }

    // ONLY CREATES LAST RESORT KEY PACKAGES
    // TODO: Implement key package rotation https://github.com/xmtp/libxmtp/issues/293
    pub(crate) fn new_key_package(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<KeyPackage, IdentityError> {
        let last_resort = Extension::LastResort(LastResortExtension::default());
        let extensions = Extensions::single(last_resort);
        let capabilities = Capabilities::new(
            None,
            Some(&[CIPHERSUITE]),
            Some(&[ExtensionType::LastResort]),
            None,
            None,
        );
        // TODO: Set expiration
        let kp = KeyPackage::builder()
            .leaf_node_capabilities(capabilities)
            .key_package_extensions(extensions)
            .build(
                CryptoConfig {
                    ciphersuite: CIPHERSUITE,
                    version: ProtocolVersion::default(),
                },
                provider,
                &self.installation_keys,
                CredentialWithKey {
                    credential: self.credential.clone(),
                    signature_key: self.installation_keys.to_public_vec().into(),
                },
            )?;

        Ok(kp)
    }

    fn create_credential(
        installation_keys: &SignatureKeyPair,
        owner: &impl InboxOwner,
    ) -> Result<Credential, IdentityError> {
        // Generate association
        let iso8601_time = format!("{}", Utc::now().format("%+"));
        let assoc_text = AssociationText::new_static(
            AssociationContext::GrantMessagingAccess,
            owner.get_address(),
            installation_keys.to_public_vec(),
            iso8601_time,
        );
        let signature = owner.sign(&assoc_text.text())?;
        let association =
            Eip191Association::new(installation_keys.public(), assoc_text, signature)?;
        // TODO wrap in a Credential proto to allow flexibility for different association types
        let association_proto: Eip191AssociationProto = association.into();

        // Serialize into credential
        Ok(Credential::new(association_proto.encode_to_vec(), CredentialType::Basic).unwrap())
    }

    pub(crate) fn get_validated_account_address(
        credential: &[u8],
        installation_public_key: &[u8],
    ) -> Result<String, IdentityError> {
        let proto = Eip191AssociationProto::decode(credential)?;
        let expected_account_address = proto.account_address.clone();
        let association = Eip191Association::from_proto_with_expected_address(
            AssociationContext::GrantMessagingAccess,
            installation_public_key,
            proto,
            expected_account_address,
        )?;

        Ok(association.address())
    }
}

#[cfg(test)]
mod tests {
    use openmls::prelude::ExtensionType;
    use xmtp_cryptography::utils::generate_local_wallet;

    use super::Identity;
    use crate::{storage::EncryptedMessageStore, xmtp_openmls_provider::XmtpOpenMlsProvider};

    #[test]
    fn does_not_error() {
        let store = EncryptedMessageStore::new_test();
        let conn = &store.conn().unwrap();
        let provider = XmtpOpenMlsProvider::new(conn);
        Identity::new(&provider, &generate_local_wallet()).unwrap();
    }

    #[test]
    fn test_key_package_extensions() {
        let store = EncryptedMessageStore::new_test();
        let conn = &store.conn().unwrap();
        let provider = XmtpOpenMlsProvider::new(conn);
        let identity = Identity::new(&provider, &generate_local_wallet()).unwrap();

        let new_key_package = identity.new_key_package(&provider).unwrap();
        assert!(new_key_package
            .extensions()
            .contains(ExtensionType::LastResort));
        assert!(new_key_package.last_resort())
    }
}
