use std::sync::RwLock;

use log::info;
use openmls::{
    credentials::errors::CredentialError,
    extensions::{errors::InvalidExtensionError, ApplicationIdExtension, LastResortExtension},
    prelude::{
        tls_codec::{Error as TlsCodecError, Serialize},
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
    api_client::XmtpMlsClient, xmtp::mls::message_contents::MlsCredential as CredentialProto,
};

use crate::{
    api_client_wrapper::{ApiClientWrapper, IdentityUpdate},
    configuration::CIPHERSUITE,
    credential::{AssociationError, Credential, UnsignedGrantMessagingAccessData},
    storage::{identity::StoredIdentity, StorageError},
    types::Address,
    utils::time::now_ns,
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Fetch, Store,
};

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("generating new identity: {0}")]
    BadGeneration(#[from] SignatureError),
    #[error("bad association: {0}")]
    BadAssocation(#[from] AssociationError),
    #[error("generating key-pairs: {0}")]
    KeyGenerationError(#[from] CryptoError),
    #[error("storage error: {0}")]
    StorageError(#[from] StorageError),
    #[error("generating key package: {0}")]
    KeyPackageGenerationError(#[from] KeyPackageNewError<StorageError>),
    #[error("deserialization: {0}")]
    Deserialization(#[from] prost::DecodeError),
    #[error("invalid extension: {0}")]
    InvalidExtension(#[from] InvalidExtensionError),
    #[error("uninitialized identity")]
    UninitializedIdentity,
    #[error("wallet signature required - please sign the text produced by text_to_sign()")]
    WalletSignatureRequired,
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error("api error: {0}")]
    ApiError(#[from] xmtp_proto::api_client::Error),
    #[error("OpenMLS credential error: {0}")]
    OpenMlsCredentialError(#[from] CredentialError),
}

#[derive(Debug)]
pub struct Identity {
    pub(crate) account_address: Address,
    pub(crate) installation_keys: SignatureKeyPair,
    pub(crate) credential: RwLock<Option<OpenMlsCredential>>,
    pub(crate) unsigned_association_data: Option<UnsignedGrantMessagingAccessData>,
}

impl Identity {
    // Creates a credential that is not yet wallet signed. Implementors should sign the payload returned by 'text_to_sign'
    // and call 'register' with the signature.
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

    // Create a credential derived from an existing wallet-signed v2 key. No additional signing needed, so 'text_to_sign' will return None.
    pub(crate) fn create_from_legacy(
        account_address: String,
        legacy_signed_private_key: Vec<u8>,
    ) -> Result<Self, IdentityError> {
        info!("Creating identity from legacy key");
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        let credential =
            Credential::create_from_legacy(&signature_keys, legacy_signed_private_key)?;
        let credential_proto: CredentialProto = credential.into();
        let mls_credential =
            OpenMlsCredential::new(CredentialType::Basic, credential_proto.encode_to_vec());
        info!("Successfully created identity from legacy key");
        Ok(Self {
            account_address,
            installation_keys: signature_keys,
            credential: RwLock::new(Some(mls_credential)),
            unsigned_association_data: None,
        })
    }

    pub(crate) async fn register<ApiClient: XmtpMlsClient>(
        &self,
        provider: &XmtpOpenMlsProvider<'_>,
        api_client: &ApiClientWrapper<ApiClient>,
        recoverable_wallet_signature: Option<Vec<u8>>,
    ) -> Result<(), IdentityError> {
        // Do not re-register if already registered
        let stored_identity: Option<StoredIdentity> = provider.conn().fetch(&())?;
        if stored_identity.is_some() {
            info!("Identity already registered, skipping registration");
            return Ok(());
        }

        info!("Registering identity");
        // If we do not have a signed credential, apply the provided signature
        if self.credential().is_err() {
            if recoverable_wallet_signature.is_none() {
                return Err(IdentityError::WalletSignatureRequired);
            }

            let credential_proto: CredentialProto = Credential::create_from_external_signer(
                self.unsigned_association_data
                    .clone()
                    .expect("Unsigned identity is always created with unsigned_association_data"),
                recoverable_wallet_signature.unwrap(),
            )?
            .into();
            let credential =
                OpenMlsCredential::new(CredentialType::Basic, credential_proto.encode_to_vec());
            self.set_credential(credential)?;
        }

        // Register the installation with the server
        let kp = self.new_key_package(provider)?;
        let kp_bytes = kp.tls_serialize_detached()?;
        api_client.register_installation(kp_bytes).await?;

        // Only persist the installation keys if the registration was successful
        self.installation_keys.store(provider.key_store())?;
        StoredIdentity::from(self).store(provider.conn())?;

        Ok(())
    }

    pub(crate) fn credential(&self) -> Result<OpenMlsCredential, IdentityError> {
        self.credential
            .read()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
            .ok_or(IdentityError::UninitializedIdentity)
    }

    fn set_credential(&self, credential: OpenMlsCredential) -> Result<(), IdentityError> {
        let mut credential_opt = self
            .credential
            .write()
            .unwrap_or_else(|err| err.into_inner());
        *credential_opt = Some(credential);
        Ok(())
    }

    pub(crate) fn text_to_sign(&self) -> Option<String> {
        if self.credential().is_ok() {
            return None;
        }
        self.unsigned_association_data
            .clone()
            .map(|data| data.text())
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

    pub(crate) async fn has_existing_legacy_credential<ApiClient: XmtpMlsClient>(
        api_client: &ApiClientWrapper<ApiClient>,
        account_address: &str,
    ) -> Result<bool, IdentityError> {
        let identity_updates = api_client
            .get_identity_updates(0 /*start_time_ns*/, vec![account_address.to_string()])
            .await?;
        if let Some(updates) = identity_updates.get(account_address) {
            for update in updates {
                let IdentityUpdate::NewInstallation(registration) = update else {
                    continue;
                };
                let Ok(proto) = CredentialProto::decode(registration.credential_bytes.as_slice())
                else {
                    continue;
                };
                let Ok(credential) = Credential::from_proto_validated(
                    proto,
                    Some(account_address), // expected_account_address
                    None,                  // expected_installation_public_key
                ) else {
                    continue;
                };
                if let Credential::LegacyCreateIdentity(_) = credential {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use ethers::signers::Signer;
    use openmls::prelude::ExtensionType;
    use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_proto::api_client::XmtpMlsClient;

    use super::Identity;
    use crate::{
        api_client_wrapper::{tests::get_test_api_client, ApiClientWrapper},
        storage::EncryptedMessageStore,
        xmtp_openmls_provider::XmtpOpenMlsProvider,
        InboxOwner,
    };

    pub async fn create_registered_identity<ApiClient: XmtpMlsClient>(
        provider: &XmtpOpenMlsProvider<'_>,
        api_client: &ApiClientWrapper<ApiClient>,
        owner: &impl InboxOwner,
    ) -> Identity {
        let identity = Identity::create_to_be_signed(owner.get_address()).unwrap();
        let signature: Option<Vec<u8>> = identity
            .text_to_sign()
            .map(|text_to_sign| owner.sign(&text_to_sign).unwrap().into());
        identity
            .register(provider, api_client, signature)
            .await
            .unwrap();
        identity
    }

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
        let _identity =
            create_registered_identity(&provider, &api_client, &generate_local_wallet()).await;
    }

    #[tokio::test]
    async fn test_key_package_extensions() {
        let (store, api_client) = get_test_resources().await;
        let conn = store.conn().unwrap();
        let provider = XmtpOpenMlsProvider::new(&conn);
        let identity =
            create_registered_identity(&provider, &api_client, &generate_local_wallet()).await;

        let new_key_package = identity.new_key_package(&provider).unwrap();
        assert!(new_key_package
            .extensions()
            .contains(ExtensionType::LastResort));
        assert!(new_key_package.last_resort())
    }

    #[tokio::test]
    async fn test_duplicate_registration() {
        let (store, api_client) = get_test_resources().await;
        let conn = store.conn().unwrap();
        let provider = XmtpOpenMlsProvider::new(&conn);
        let identity =
            create_registered_identity(&provider, &api_client, &generate_local_wallet()).await;
        identity
            .register(&provider, &api_client, None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_legacy_identity() {
        let legacy_address = "0x419cb1fa5635b0c6df47c9dc5765c8f1f4dff78e";
        let legacy_signed_private_key_proto = vec![
            8, 128, 154, 196, 133, 220, 244, 197, 216, 23, 18, 34, 10, 32, 214, 70, 104, 202, 68,
            204, 25, 202, 197, 141, 239, 159, 145, 249, 55, 242, 147, 126, 3, 124, 159, 207, 96,
            135, 134, 122, 60, 90, 82, 171, 131, 162, 26, 153, 1, 10, 79, 8, 128, 154, 196, 133,
            220, 244, 197, 216, 23, 26, 67, 10, 65, 4, 232, 32, 50, 73, 113, 99, 115, 168, 104,
            229, 206, 24, 217, 132, 223, 217, 91, 63, 137, 136, 50, 89, 82, 186, 179, 150, 7, 127,
            140, 10, 165, 117, 233, 117, 196, 134, 227, 143, 125, 210, 187, 77, 195, 169, 162, 116,
            34, 20, 196, 145, 40, 164, 246, 139, 197, 154, 233, 190, 148, 35, 131, 240, 106, 103,
            18, 70, 18, 68, 10, 64, 90, 24, 36, 99, 130, 246, 134, 57, 60, 34, 142, 165, 221, 123,
            63, 27, 138, 242, 195, 175, 212, 146, 181, 152, 89, 48, 8, 70, 104, 94, 163, 0, 25,
            196, 228, 190, 49, 108, 141, 60, 174, 150, 177, 115, 229, 138, 92, 105, 170, 226, 204,
            249, 206, 12, 37, 145, 3, 35, 226, 15, 49, 20, 102, 60, 16, 1,
        ];
        let (store, api_client) = get_test_resources().await;
        let conn = store.conn().unwrap();
        let provider = XmtpOpenMlsProvider::new(&conn);
        let identity = Identity::create_from_legacy(
            legacy_address.to_string(),
            legacy_signed_private_key_proto,
        )
        .unwrap();
        assert!(identity.text_to_sign().is_none());
        identity
            .register(&provider, &api_client, None)
            .await
            .unwrap();
        assert_eq!(identity.account_address, legacy_address);
    }

    #[tokio::test]
    async fn test_invalid_external_signature() {
        let (store, api_client) = get_test_resources().await;
        let conn = store.conn().unwrap();
        let provider = XmtpOpenMlsProvider::new(&conn);
        let wallet = generate_local_wallet();
        let identity = Identity::create_to_be_signed(wallet.get_address()).unwrap();
        let text_to_sign = identity.text_to_sign().unwrap();
        let mut signature = wallet.sign_message(text_to_sign).await.unwrap().to_vec();
        signature[0] ^= 1; // Tamper with signature
        assert!(identity
            .register(&provider, &api_client, Some(signature))
            .await
            .is_err());
    }
}
