use std::array::TryFromSliceError;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::configuration::GROUP_PERMISSIONS_EXTENSION_ID;
use crate::retry::RetryableError;
use crate::storage::db_connection::DbConnection;
use crate::storage::identity::StoredIdentity;
use crate::storage::sql_key_store::{SqlKeyStore, SqlKeyStoreError, KEY_PACKAGE_REFERENCES};
use crate::storage::EncryptedMessageStore;
use crate::{
    api::{ApiClientWrapper, WrappedApiError},
    configuration::{CIPHERSUITE, GROUP_MEMBERSHIP_EXTENSION_ID, MUTABLE_METADATA_EXTENSION_ID},
    storage::StorageError,
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    XmtpApi,
};
use crate::{retryable, Fetch, Store};
use ed25519_dalek::SigningKey;
use openmls::prelude::hash_ref::HashReference;
use openmls::prelude::tls_codec::Serialize;
use openmls::{
    credentials::{errors::BasicCredentialError, BasicCredential, CredentialWithKey},
    extensions::{
        ApplicationIdExtension, Extension, ExtensionType, Extensions, LastResortExtension,
    },
    messages::proposals::ProposalType,
    prelude::{Capabilities, Credential as OpenMlsCredential},
    prelude_test::KeyPackage,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::storage::StorageProvider;
use openmls_traits::types::CryptoError;
use openmls_traits::OpenMlsProvider;
use prost::Message;
use sha2::{Digest, Sha512};
use thiserror::Error;
use tracing::debug;
use tracing::info;
use xmtp_id::associations::unverified::{UnverifiedInstallationKeySignature, UnverifiedSignature};
use xmtp_id::associations::AssociationError;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::{
    associations::{
        builder::{SignatureRequest, SignatureRequestBuilder, SignatureRequestError},
        generate_inbox_id, sign_with_legacy_key, MemberIdentifier,
    },
    constants::INSTALLATION_KEY_SIGNATURE_CONTEXT,
    InboxId,
};
use xmtp_proto::xmtp::identity::MlsCredential;

/**
 * The identity strategy determines how the [`ClientBuilder`] constructs an identity on startup.
 *
 * [`IdentityStrategy::CreateIfNotFound`] will attempt to create a new identity if one isn't found in the store.
 * This is the default behavior.
 *
 * [`IdentityStrategy::CachedOnly`] will attempt to get an identity from the store. If not found, it will
 * return an error. This is useful if you don't want to create a new identity on startup because the caller
 * does not have access to a signer.
 *
 * [`IdentityStrategy::ExternalIdentity`] allows you to provide an already-constructed identity to the
 * client. This is useful for testing and not expected to be used in production.
 */
#[derive(Debug, Clone)]
pub enum IdentityStrategy {
    /// Tries to get an identity from the disk store. If not found, getting one from backend.
    CreateIfNotFound(InboxId, String, u64, Option<Vec<u8>>), // (inbox_id, address, nonce, legacy_signed_private_key)
    /// Identity that is already in the disk store
    CachedOnly,
    /// An already-built Identity for testing purposes
    #[cfg(test)]
    ExternalIdentity(Identity),
}

impl IdentityStrategy {
    /**
     * Initialize an identity from the given strategy. If a stored identity is found in the database,
     * it will return that identity.
     *
     * If a stored identity is found, it will validate that the inbox_id of the stored identity matches
     * the inbox_id configured on the strategy.
     *
     **/
    pub(crate) async fn initialize_identity<ApiClient: XmtpApi>(
        self,
        api_client: &ApiClientWrapper<ApiClient>,
        store: &EncryptedMessageStore,
        scw_signature_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<Identity, IdentityError> {
        info!("Initializing identity");
        let conn = store.conn()?;
        let provider = XmtpOpenMlsProvider::new(conn);
        let stored_identity: Option<Identity> = provider
            .conn_ref()
            .fetch(&())?
            .map(|i: StoredIdentity| i.try_into())
            .transpose()?;

        debug!("identity in store: {:?}", stored_identity);
        match self {
            IdentityStrategy::CachedOnly => {
                stored_identity.ok_or(IdentityError::RequiredIdentityNotFound)
            }
            IdentityStrategy::CreateIfNotFound(
                inbox_id,
                address,
                nonce,
                legacy_signed_private_key,
            ) => {
                if let Some(stored_identity) = stored_identity {
                    if inbox_id != stored_identity.inbox_id {
                        return Err(IdentityError::InboxIdMismatch {
                            id: inbox_id.clone(),
                            stored: stored_identity.inbox_id,
                        });
                    }

                    Ok(stored_identity)
                } else {
                    Identity::new(
                        inbox_id,
                        address,
                        nonce,
                        legacy_signed_private_key,
                        api_client,
                        &provider,
                        scw_signature_verifier,
                    )
                    .await
                }
            }
            #[cfg(test)]
            IdentityStrategy::ExternalIdentity(identity) => Ok(identity),
        }
    }
}

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error(transparent)]
    CredentialSerialization(#[from] prost::EncodeError),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error(transparent)]
    WrappedApi(#[from] WrappedApiError),
    #[error(transparent)]
    Api(#[from] xmtp_proto::api_client::Error),
    #[error("installation not found: {0}")]
    InstallationIdNotFound(String),
    #[error(transparent)]
    SignatureRequestBuilder(#[from] SignatureRequestError),
    #[error(transparent)]
    Signature(#[from] xmtp_id::associations::SignatureError),
    #[error(transparent)]
    BasicCredential(#[from] BasicCredentialError),
    #[error("Legacy key re-use")]
    LegacyKeyReuse,
    #[error("Uninitialized identity")]
    UninitializedIdentity,
    #[error("Installation key {0}")]
    InstallationKey(String),
    #[error("Malformed legacy key: {0}")]
    MalformedLegacyKey(String),
    #[error("Legacy signature: {0}")]
    LegacySignature(String),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error("legacy key does not match address")]
    LegacyKeyMismatch,
    #[error(transparent)]
    OpenMls(#[from] openmls::prelude::Error),
    #[error(transparent)]
    StorageError(#[from] crate::storage::StorageError),
    #[error(transparent)]
    OpenMlsStorageError(#[from] SqlKeyStoreError),
    #[error(transparent)]
    KeyPackageGenerationError(#[from] openmls::key_packages::errors::KeyPackageNewError),
    #[error(transparent)]
    ED25519Error(#[from] ed25519_dalek::ed25519::Error),
    #[error("The InboxID {id}, associated does not match the stored InboxId {stored}.")]
    InboxIdMismatch { id: InboxId, stored: InboxId },
    #[error("The address {0} has no associated InboxID")]
    NoAssociatedInboxId(String),
    #[error("Required identity was not found in cache.")]
    RequiredIdentityNotFound,
    #[error("error creating new identity: {0}")]
    NewIdentity(String),
    #[error(transparent)]
    DieselResult(#[from] diesel::result::Error),
    #[error(transparent)]
    Association(#[from] AssociationError),
}

impl RetryableError for IdentityError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Api(_) => true,
            Self::WrappedApi(err) => retryable!(err),
            Self::StorageError(err) => retryable!(err),
            Self::OpenMlsStorageError(err) => retryable!(err),
            Self::DieselResult(err) => retryable!(err),
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct Identity {
    pub(crate) inbox_id: InboxId,
    pub(crate) installation_keys: SignatureKeyPair,
    pub(crate) credential: OpenMlsCredential,
    pub(crate) signature_request: Option<SignatureRequest>,
    pub(crate) is_ready: AtomicBool,
}

impl Clone for Identity {
    fn clone(&self) -> Self {
        Self {
            inbox_id: self.inbox_id.clone(),
            installation_keys: self.installation_keys.clone(),
            credential: self.credential.clone(),
            signature_request: self.signature_request(),
            is_ready: AtomicBool::new(self.is_ready.load(Ordering::SeqCst)),
        }
    }
}

impl Identity {
    /// Create a new [Identity] instance.
    ///
    /// If the address is already associated with an inbox_id, the existing inbox_id will be used.
    /// Users will be required to sign with their wallet, and the legacy is ignored even if it's provided.
    ///
    /// If the address is NOT associated with an inbox_id, a new inbox_id will be generated.
    /// If a legacy key is provided, it will be used to sign the identity update and no wallet signature is needed.
    ///
    /// If no legacy key is provided, a wallet signature is always required.
    pub(crate) async fn new<ApiClient: XmtpApi>(
        inbox_id: InboxId,
        address: String,
        nonce: u64,
        legacy_signed_private_key: Option<Vec<u8>>,
        api_client: &ApiClientWrapper<ApiClient>,
        provider: &XmtpOpenMlsProvider,
        scw_signature_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<Self, IdentityError> {
        // check if address is already associated with an inbox_id
        let address = address.to_lowercase();
        let inbox_ids = api_client.get_inbox_ids(vec![address.clone()]).await?;
        let associated_inbox_id = inbox_ids.get(&address);
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        let installation_public_key = signature_keys.public();
        let member_identifier: MemberIdentifier = address.clone().to_lowercase().into();

        if let Some(associated_inbox_id) = associated_inbox_id {
            // If an inbox is associated with address, we'd use it to create Identity and ignore the nonce.
            // We would need a signature from user's wallet.
            if *associated_inbox_id != inbox_id {
                return Err(IdentityError::NewIdentity("Inbox ID mismatch".to_string()));
            }
            let builder = SignatureRequestBuilder::new(associated_inbox_id.clone());
            let mut signature_request = builder
                .add_association(installation_public_key.to_vec().into(), member_identifier)
                .build();

            signature_request
                .add_signature(
                    UnverifiedSignature::InstallationKey(
                        sign_with_installation_key(
                            signature_request.signature_text(),
                            sized_installation_key(signature_keys.private())?,
                        )
                        .await?,
                    ),
                    scw_signature_verifier,
                )
                .await?;

            let identity = Self {
                inbox_id: associated_inbox_id.clone(),
                installation_keys: signature_keys,
                credential: create_credential(associated_inbox_id.clone())?,
                signature_request: Some(signature_request),
                is_ready: AtomicBool::new(false),
            };

            Ok(identity)
        } else if let Some(legacy_signed_private_key) = legacy_signed_private_key {
            // The legacy signed private key may only be used if the nonce is 0
            if nonce != 0 {
                return Err(IdentityError::NewIdentity(
                    "Nonce must be 0 if legacy key is provided".to_string(),
                ));
            }
            // If the inbox_id found on the network does not match the one generated from the address and nonce, we must error
            let generated_inbox_id = generate_inbox_id(&address, &nonce)?;
            if inbox_id != generated_inbox_id {
                return Err(IdentityError::NewIdentity(
                    "Inbox ID doesn't match nonce & address".to_string(),
                ));
            }
            let mut builder = SignatureRequestBuilder::new(inbox_id.clone());
            builder = builder.create_inbox(member_identifier.clone(), nonce);
            let mut signature_request = builder
                .add_association(installation_public_key.to_vec().into(), member_identifier)
                .build();

            signature_request
                .add_signature(
                    UnverifiedSignature::InstallationKey(
                        sign_with_installation_key(
                            signature_request.signature_text(),
                            sized_installation_key(signature_keys.private())?,
                        )
                        .await?,
                    ),
                    &scw_signature_verifier,
                )
                .await?;
            signature_request
                .add_signature(
                    UnverifiedSignature::LegacyDelegated(
                        sign_with_legacy_key(
                            signature_request.signature_text(),
                            legacy_signed_private_key,
                        )
                        .await?,
                    ),
                    scw_signature_verifier,
                )
                .await?;

            // Make sure to register the identity before applying the signature request
            let identity = Self {
                inbox_id: inbox_id.clone(),
                installation_keys: signature_keys,
                credential: create_credential(inbox_id)?,
                signature_request: None,
                is_ready: AtomicBool::new(true),
            };

            identity.register(provider, api_client).await?;

            let identity_update = signature_request.build_identity_update()?;
            api_client.publish_identity_update(identity_update).await?;

            Ok(identity)
        } else {
            let generated_inbox_id = generate_inbox_id(&address, &nonce)?;
            if inbox_id != generated_inbox_id {
                return Err(IdentityError::NewIdentity(
                    "Inbox ID doesn't match nonce & address".to_string(),
                ));
            }
            let mut builder = SignatureRequestBuilder::new(inbox_id.clone());
            builder = builder.create_inbox(member_identifier.clone(), nonce);

            let mut signature_request = builder
                .add_association(installation_public_key.to_vec().into(), member_identifier)
                .build();

            // We can pre-sign the request with an installation key signature, since we have access to the key
            signature_request
                .add_signature(
                    UnverifiedSignature::InstallationKey(
                        sign_with_installation_key(
                            signature_request.signature_text(),
                            sized_installation_key(signature_keys.private())?,
                        )
                        .await?,
                    ),
                    scw_signature_verifier,
                )
                .await?;

            let identity = Self {
                inbox_id: inbox_id.clone(),
                installation_keys: signature_keys,
                credential: create_credential(inbox_id.clone())?,
                signature_request: Some(signature_request),
                is_ready: AtomicBool::new(false),
            };

            Ok(identity)
        }
    }

    pub fn inbox_id(&self) -> &InboxId {
        &self.inbox_id
    }

    pub fn sequence_id(&self, conn: &DbConnection) -> Result<i64, StorageError> {
        conn.get_latest_sequence_id_for_inbox(self.inbox_id.as_str())
    }

    #[allow(dead_code)]
    pub fn is_ready(&self) -> bool {
        self.is_ready.load(Ordering::SeqCst)
    }

    pub fn signature_request(&self) -> Option<SignatureRequest> {
        self.signature_request.clone()
    }

    pub fn credential(&self) -> OpenMlsCredential {
        self.credential.clone()
    }

    /**
     * Sign the given text with the installation private key.
     */
    pub(crate) fn sign<Text: AsRef<str>>(&self, text: Text) -> Result<Vec<u8>, IdentityError> {
        let mut prehashed = Sha512::new();
        prehashed.update(text.as_ref());
        let k = ed25519_dalek::SigningKey::try_from(self.installation_keys.private())
            .expect("signing key is invalid");
        let signature = k.sign_prehashed(prehashed, Some(INSTALLATION_KEY_SIGNATURE_CONTEXT))?;
        Ok(signature.to_vec())
    }

    /// Generate a new key package and store the associated keys in the database.
    pub(crate) fn new_key_package(
        &self,
        provider: impl OpenMlsProvider<StorageProvider = SqlKeyStore<crate::storage::RawDbConnection>>,
    ) -> Result<KeyPackage, IdentityError> {
        let last_resort = Extension::LastResort(LastResortExtension::default());
        let key_package_extensions = Extensions::single(last_resort);

        let application_id =
            Extension::ApplicationId(ApplicationIdExtension::new(self.inbox_id().as_bytes()));
        let leaf_node_extensions = Extensions::single(application_id);

        let capabilities = Capabilities::new(
            None,
            Some(&[CIPHERSUITE]),
            Some(&[
                ExtensionType::LastResort,
                ExtensionType::ApplicationId,
                ExtensionType::Unknown(GROUP_PERMISSIONS_EXTENSION_ID),
                ExtensionType::Unknown(MUTABLE_METADATA_EXTENSION_ID),
                ExtensionType::Unknown(GROUP_MEMBERSHIP_EXTENSION_ID),
                ExtensionType::ImmutableMetadata,
            ]),
            Some(&[ProposalType::GroupContextExtensions]),
            None,
        );
        let kp = KeyPackage::builder()
            .leaf_node_capabilities(capabilities)
            .leaf_node_extensions(leaf_node_extensions)
            .key_package_extensions(key_package_extensions)
            .build(
                CIPHERSUITE,
                &provider,
                &self.installation_keys,
                CredentialWithKey {
                    credential: self.credential(),
                    signature_key: self.installation_keys.to_public_vec().into(),
                },
            )?;
        // Store the hash reference, keyed with the public init key.
        // This is needed to get to the private key when decrypting welcome messages.
        let public_init_key = kp.key_package().hpke_init_key().tls_serialize_detached()?;

        let hash_ref = serialize_key_package_hash_ref(kp.key_package(), &provider)?;
        // Store the hash reference, keyed with the public init key
        provider
            .storage()
            .write::<{ openmls_traits::storage::CURRENT_VERSION }>(
                KEY_PACKAGE_REFERENCES,
                &public_init_key,
                &hash_ref,
            )?;
        Ok(kp.key_package().clone())
    }

    pub(crate) async fn register<ApiClient: XmtpApi>(
        &self,
        provider: &XmtpOpenMlsProvider,
        api_client: &ApiClientWrapper<ApiClient>,
    ) -> Result<(), IdentityError> {
        let stored_identity: Option<StoredIdentity> = provider.conn_ref().fetch(&())?;
        if stored_identity.is_some() {
            info!("Identity already registered. skipping key package publishing");
            return Ok(());
        }

        self.rotate_key_package(provider, api_client).await?;
        self.is_ready.store(true, Ordering::SeqCst);

        Ok(StoredIdentity::try_from(self)?.store(provider.conn_ref())?)
    }

    /// Upload a new key package to the network, which will replace any existing key packages for the installation.
    pub(crate) async fn rotate_key_package<ApiClient: XmtpApi>(
        &self,
        provider: &XmtpOpenMlsProvider,
        api_client: &ApiClientWrapper<ApiClient>,
    ) -> Result<(), IdentityError> {
        let kp = self.new_key_package(provider)?;
        let kp_bytes = kp.tls_serialize_detached()?;
        let conn = provider.conn_ref();
        let hash_ref = serialize_key_package_hash_ref(&kp, provider)?;
        let history_id = conn.store_key_package_history_entry(hash_ref)?.id;
        let old_id = history_id - 1;

        // Find all key packages that are not the current or previous KPs
        // We can delete before uploading because this is either run inside a transaction or is being applied to a brand
        // new identity
        let old_key_packages = conn.find_key_package_history_entries_before_id(old_id)?;
        for kp in old_key_packages {
            self.delete_key_package(provider, kp.key_package_hash_ref)?;
        }
        conn.delete_key_package_history_entries_before_id(old_id)?;

        api_client.upload_key_package(kp_bytes, true).await?;
        Ok(())
    }

    /// Delete a key package from the local database.
    pub(crate) fn delete_key_package(
        &self,
        provider: &XmtpOpenMlsProvider,
        hash_ref: Vec<u8>,
    ) -> Result<(), IdentityError> {
        let openmls_hash_ref = deserialize_key_package_hash_ref(&hash_ref)?;
        provider.storage().delete_key_package(&openmls_hash_ref)?;

        Ok(())
    }
}

pub(crate) fn serialize_key_package_hash_ref(
    kp: &KeyPackage,
    provider: &impl OpenMlsProvider<StorageProvider = SqlKeyStore<crate::storage::RawDbConnection>>,
) -> Result<Vec<u8>, IdentityError> {
    let key_package_hash_ref = kp
        .hash_ref(provider.crypto())
        .map_err(|_| IdentityError::UninitializedIdentity)?;
    let serialized = bincode::serialize(&key_package_hash_ref)
        .map_err(|_| IdentityError::UninitializedIdentity)?;

    Ok(serialized)
}

fn deserialize_key_package_hash_ref(hash_ref: &[u8]) -> Result<HashReference, IdentityError> {
    let key_package_hash_ref: HashReference =
        bincode::deserialize(hash_ref).map_err(|_| IdentityError::UninitializedIdentity)?;

    Ok(key_package_hash_ref)
}

async fn sign_with_installation_key(
    signature_text: String,
    installation_private_key: &[u8; 32],
) -> Result<UnverifiedInstallationKeySignature, IdentityError> {
    let signing_key: SigningKey = SigningKey::from_bytes(installation_private_key);
    let verifying_key = signing_key.verifying_key();
    let mut prehashed: Sha512 = Sha512::new();
    prehashed.update(signature_text.clone());
    let sig = signing_key.sign_prehashed(prehashed, Some(INSTALLATION_KEY_SIGNATURE_CONTEXT))?;
    let unverified_sig =
        UnverifiedInstallationKeySignature::new(sig.to_vec(), verifying_key.as_bytes().to_vec());

    Ok(unverified_sig)
}

fn sized_installation_key(installation_key: &[u8]) -> Result<&[u8; 32], IdentityError> {
    installation_key
        .try_into()
        .map_err(|e: TryFromSliceError| IdentityError::InstallationKey(e.to_string()))
}

fn create_credential(inbox_id: InboxId) -> Result<OpenMlsCredential, IdentityError> {
    let cred = MlsCredential { inbox_id };
    let mut credential_bytes = Vec::new();
    let _ = cred.encode(&mut credential_bytes);

    Ok(BasicCredential::new(credential_bytes).into())
}

pub fn parse_credential(credential_bytes: &[u8]) -> Result<InboxId, IdentityError> {
    let cred = MlsCredential::decode(credential_bytes)?;
    Ok(cred.inbox_id)
}
