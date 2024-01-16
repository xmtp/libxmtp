use openmls::{
    extensions::{errors::InvalidExtensionError, ApplicationIdExtension, LastResortExtension},
    prelude::{
        Capabilities, Credential as OpenMlsCredential, CredentialType, CredentialWithKey,
        CryptoConfig, Extension, ExtensionType, Extensions, KeyPackage, KeyPackageNewError,
        Lifetime,
    },
    versions::ProtocolVersion,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::{types::CryptoError, OpenMlsProvider};
use prost::Message;
use thiserror::Error;
use xmtp_cryptography::signature::SignatureError;
use xmtp_proto::{
    api_client::{XmtpApiClient, XmtpMlsClient},
    xmtp::mls::message_contents::MlsCredential as CredentialProto,
};

use crate::{
    api_client_wrapper::ApiClientWrapper,
    builder::LegacyIdentitySource,
    configuration::CIPHERSUITE,
    credential::{AssociationError, Credential},
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
    #[error("invalid extension")]
    InvalidExtension(#[from] InvalidExtensionError),
    #[error("network error")]
    Network(#[from] xmtp_proto::api_client::Error),
}

#[derive(Debug)]
pub struct Identity {
    pub(crate) account_address: Address,
    pub(crate) installation_keys: SignatureKeyPair,
    pub(crate) credential: OpenMlsCredential,
}

impl Identity {
    pub(crate) async fn new<ApiClient: XmtpApiClient + XmtpMlsClient>(
        api_client: &ApiClientWrapper<ApiClient>,
        provider: &XmtpOpenMlsProvider<'_>,
        owner: &impl InboxOwner,
        legacy_identity_source: LegacyIdentitySource,
    ) -> Result<Self, IdentityError> {
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        signature_keys.store(provider.key_store())?;

        let credential = match legacy_identity_source {
            LegacyIdentitySource::None | LegacyIdentitySource::Network => {
                Identity::create_credential(&signature_keys, owner)?
            }
            LegacyIdentitySource::Static(v2_signed_private_key)
            | LegacyIdentitySource::KeyGenerator(v2_signed_private_key) => {
                // Check if a v2-signed key already exists
                let identity_updates = api_client
                    .get_identity_updates(0 /*start_time_ns*/, vec![owner.get_address()])
                    .await?;
                // - we need an API client. where is this being uploaded?
                // If so, use it to create a new credential
                todo!()
            }
        };

        let identity = Self {
            account_address: owner.get_address(),
            installation_keys: signature_keys,
            credential,
        };

        StoredIdentity::from(&identity).store(provider.conn())?;

        Ok(identity)
    }

    // ONLY CREATES LAST RESORT KEY PACKAGES
    pub(crate) fn new_key_package(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<KeyPackage, IdentityError> {
        let last_resort = Extension::LastResort(LastResortExtension::default());
        let key_package_extensions = Extensions::single(last_resort);

        let application_id =
            Extension::ApplicationId(ApplicationIdExtension::new(self.account_address.as_bytes()));
        let leaf_node_extensions = Extensions::single(application_id);

        let capabilities = Capabilities::new(
            None,
            Some(&[CIPHERSUITE]),
            Some(&[ExtensionType::LastResort, ExtensionType::ApplicationId]),
            None,
            None,
        );
        let kp = KeyPackage::builder()
            .leaf_node_capabilities(capabilities)
            .leaf_node_extensions(leaf_node_extensions)
            .key_package_extensions(key_package_extensions)
            .key_package_lifetime(Lifetime::new(6 * 30 * 86400))
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
    ) -> Result<OpenMlsCredential, IdentityError> {
        let credential = Credential::create(installation_keys, owner)?;
        let credential_proto: CredentialProto = credential.into();
        Ok(
            OpenMlsCredential::new(credential_proto.encode_to_vec(), CredentialType::Basic)
                .unwrap(),
        )
    }

    pub(crate) fn get_validated_account_address(
        credential: &[u8],
        installation_public_key: &[u8],
    ) -> Result<String, IdentityError> {
        let proto = CredentialProto::decode(credential)?;
        let credential = Credential::from_proto_validated(
            proto,
            None, // expected_account_address
            Some(installation_public_key),
        )?;

        Ok(credential.address())
    }

    pub fn application_id(&self) -> Vec<u8> {
        self.account_address.as_bytes().to_vec()
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
