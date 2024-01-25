use std::sync::RwLock;

use chrono::Utc;
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
use tls_codec::Serialize;
use xmtp_cryptography::signature::SignatureError;
use xmtp_proto::{
    api_client::XmtpMlsClient, xmtp::mls::message_contents::MlsCredential as CredentialProto,
};

use crate::{
    api_client_wrapper::ApiClientWrapper,
    association::{AssociationContext, AssociationError, AssociationText, Credential},
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
    #[error("invalid extension")]
    InvalidExtension(#[from] InvalidExtensionError),
    #[error("uninitialized identity")]
    UninitializedIdentity,
    #[error("wallet signature required - please sign the text produced by text_to_sign()")]
    WalletSignatureRequired,
    #[error("tls serialization: {0}")]
    TlsSerialization(#[from] tls_codec::Error),
    #[error("api error: {0}")]
    ApiError(#[from] xmtp_proto::api_client::Error),
}

#[derive(Debug)]
pub struct Identity {
    pub(crate) account_address: Address,
    pub(crate) installation_keys: SignatureKeyPair,
    pub(crate) credential: RwLock<Option<OpenMlsCredential>>,
}

impl Identity {
    pub(crate) fn new(owner: &impl InboxOwner) -> Result<Self, IdentityError> {
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        let credential = Identity::create_credential(&signature_keys, owner)?;

        let identity = Self {
            account_address: owner.get_address(),
            installation_keys: signature_keys,
            credential: RwLock::new(Some(credential)),
        };

        Ok(identity)
    }

    pub(crate) fn new_unsigned(account_address: String) -> Result<Self, IdentityError> {
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;

        let identity = Self {
            account_address,
            installation_keys: signature_keys,
            credential: RwLock::new(None),
        };

        Ok(identity)
    }

    pub(crate) async fn register<ApiClient: XmtpMlsClient>(
        &self,
        provider: &XmtpOpenMlsProvider<'_>,
        api_client: &ApiClientWrapper<ApiClient>,
        signature: Option<Vec<u8>>,
    ) -> Result<(), IdentityError> {
        let credential_opt = self
            .credential
            .read()
            .unwrap_or_else(|err| err.into_inner());
        if credential_opt.is_none() {
            if signature.is_none() {
                return Err(IdentityError::WalletSignatureRequired);
            }

            return Err(IdentityError::UninitializedIdentity);
        }

        self.installation_keys.store(provider.key_store())?;
        StoredIdentity::from(self).store(provider.conn())?;

        let kp = self.new_key_package(&provider)?;
        let kp_bytes = kp.tls_serialize_detached()?;

        api_client.register_installation(kp_bytes).await?;

        Ok(())
    }

    pub(crate) fn credential(&self) -> Result<OpenMlsCredential, IdentityError> {
        self.credential
            .read()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
            .ok_or(IdentityError::UninitializedIdentity)
    }

    pub(crate) fn text_to_sign(&self) -> Option<String> {
        if self.credential().is_ok() {
            return None;
        }

        let iso8601_time = format!("{}", Utc::now().format("%+"));
        return Some(
            AssociationText::new_static(
                AssociationContext::GrantMessagingAccess,
                self.account_address.clone(),
                self.installation_keys.to_public_vec(),
                iso8601_time,
            )
            .text(),
        );
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
                    credential: self.credential()?,
                    signature_key: self.installation_keys.to_public_vec().into(),
                },
            )?;

        Ok(kp)
    }

    fn create_credential(
        installation_keys: &SignatureKeyPair,
        owner: &impl InboxOwner,
    ) -> Result<OpenMlsCredential, IdentityError> {
        let credential = Credential::create_eip191(installation_keys, owner)?;
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
    use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
    use xmtp_cryptography::utils::generate_local_wallet;

    use super::Identity;
    use crate::{
        api_client_wrapper::{tests::get_test_api_client, ApiClientWrapper},
        storage::EncryptedMessageStore,
        xmtp_openmls_provider::XmtpOpenMlsProvider,
        InboxOwner,
    };

    async fn get_test_resources() -> (EncryptedMessageStore, ApiClientWrapper<GrpcClient>) {
        let store = EncryptedMessageStore::new_test();
        let api_client = get_test_api_client().await;
        (store, api_client)
    }

    #[tokio::test]
    async fn does_not_error() {
        let (store, api_client) = get_test_resources().await;
        let conn = store.conn().unwrap();
        let provider = XmtpOpenMlsProvider::new(&conn);
        let identity = Identity::new(&generate_local_wallet()).unwrap();
        identity
            .register(&provider, &api_client, None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_key_package_extensions() {
        let (store, api_client) = get_test_resources().await;
        let conn = store.conn().unwrap();
        let provider = XmtpOpenMlsProvider::new(&conn);
        let identity = Identity::new(&generate_local_wallet()).unwrap();
        identity
            .register(&provider, &api_client, None)
            .await
            .unwrap();

        let new_key_package = identity.new_key_package(&provider).unwrap();
        assert!(new_key_package
            .extensions()
            .contains(ExtensionType::LastResort));
        assert!(new_key_package.last_resort())
    }

    #[tokio::test]
    async fn identity_registration() {
        let (store, api_client) = get_test_resources().await;
        let conn = store.conn().unwrap();
        let provider = XmtpOpenMlsProvider::new(&conn);
        let identity = Identity::new_unsigned(generate_local_wallet().get_address()).unwrap();
        // identity
        //     .register(&provider, &api_client, None)
        //     .await
        //     .unwrap();
    }
}
