use crate::configuration::GROUP_PERMISSIONS_EXTENSION_ID;
use crate::storage::ProviderTransactions;
use crate::storage::db_connection::DbConnection;
use crate::storage::identity::StoredIdentity;
use crate::storage::sql_key_store::{KEY_PACKAGE_REFERENCES, SqlKeyStore, SqlKeyStoreError};
use crate::verified_key_package_v2::KeyPackageVerificationError;
use crate::{
    Fetch, Store, XmtpApi,
    configuration::{CIPHERSUITE, GROUP_MEMBERSHIP_EXTENSION_ID, MUTABLE_METADATA_EXTENSION_ID},
    storage::{StorageError, xmtp_openmls_provider::XmtpOpenMlsProvider},
};
use openmls::prelude::hash_ref::HashReference;
use openmls::{
    credentials::{BasicCredential, CredentialWithKey, errors::BasicCredentialError},
    extensions::{
        ApplicationIdExtension, Extension, ExtensionType, Extensions, LastResortExtension,
    },
    key_packages::KeyPackage,
    messages::proposals::ProposalType,
    prelude::{Capabilities, Credential as OpenMlsCredential, tls_codec::Serialize},
};
use openmls_traits::OpenMlsProvider;
use openmls_traits::storage::StorageProvider;
use openmls_traits::types::CryptoError;
use prost::Message;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use tracing::debug;
use tracing::info;
use xmtp_api::ApiClientWrapper;
use xmtp_common::{RetryableError, retryable};
use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential};
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_id::associations::{AssociationError, InstallationKeyContext, PublicContext};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::{
    InboxId, InboxIdRef,
    associations::{
        MemberIdentifier,
        builder::{SignatureRequest, SignatureRequestBuilder, SignatureRequestError},
        generate_inbox_id, sign_with_legacy_key,
    },
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
    CreateIfNotFound {
        inbox_id: InboxId,
        address: String,
        nonce: u64,
        legacy_signed_private_key: Option<Vec<u8>>,
    },
    /// Identity that is already in the disk store
    CachedOnly,
    /// An already-built Identity for testing purposes
    #[cfg(test)]
    ExternalIdentity(Identity),
}

impl IdentityStrategy {
    pub fn inbox_id(&self) -> Option<InboxIdRef<'_>> {
        use IdentityStrategy::*;
        match self {
            CreateIfNotFound { inbox_id, .. } => Some(inbox_id),
            _ => None,
        }
    }

    /// Create a new Identity Strategy, with [`IdentityStrategy::CreateIfNotFound`].
    /// If an Identity is not found in the local store, creates a new one.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn new(
        inbox_id: InboxId,
        address: String,
        nonce: u64,
        legacy_signed_private_key: Option<Vec<u8>>,
    ) -> Self {
        Self::CreateIfNotFound {
            inbox_id,
            address,
            nonce,
            legacy_signed_private_key,
        }
    }
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
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) async fn initialize_identity<ApiClient: XmtpApi>(
        self,
        api_client: &ApiClientWrapper<ApiClient>,
        provider: &XmtpOpenMlsProvider,
        scw_signature_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<Identity, IdentityError> {
        use IdentityStrategy::*;

        info!("Initializing identity");
        let stored_identity: Option<Identity> = provider
            .conn_ref()
            .fetch(&())?
            .map(|i: StoredIdentity| i.try_into())
            .transpose()?;

        debug!("identity in store: {:?}", stored_identity);
        match self {
            CachedOnly => stored_identity.ok_or(IdentityError::RequiredIdentityNotFound),
            CreateIfNotFound {
                inbox_id,
                address,
                nonce,
                legacy_signed_private_key,
            } => match stored_identity {
                Some(stored_identity) => {
                    tracing::debug!(
                        installation_id =
                            hex::encode(stored_identity.installation_keys.public_bytes()),
                        inbox_id = stored_identity.inbox_id,
                        "Found existing identity in store"
                    );
                    if inbox_id != stored_identity.inbox_id {
                        return Err(IdentityError::InboxIdMismatch {
                            id: inbox_id.clone(),
                            stored: stored_identity.inbox_id,
                        });
                    }

                    Ok(stored_identity)
                }
                _ => {
                    Identity::new(
                        inbox_id,
                        address,
                        nonce,
                        legacy_signed_private_key,
                        api_client,
                        provider,
                        scw_signature_verifier,
                    )
                    .await
                }
            },
            #[cfg(test)]
            ExternalIdentity(identity) => Ok(identity),
        }
    }
}

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error(transparent)]
    CredentialSerialization(#[from] prost::EncodeError),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
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
    KeyPackageVerificationError(#[from] KeyPackageVerificationError),
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
    #[error(transparent)]
    Signer(#[from] xmtp_cryptography::SignerError),
    #[error(transparent)]
    ApiClient(#[from] xmtp_api::Error),
}

impl RetryableError for IdentityError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::ApiClient(err) => retryable!(err),
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
    pub(crate) installation_keys: XmtpInstallationCredential,
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
    #[tracing::instrument(level = "trace", skip_all)]
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
        let installation_keys = XmtpInstallationCredential::new();
        let member_identifier: MemberIdentifier = address.clone().to_lowercase().into();

        if let Some(associated_inbox_id) = associated_inbox_id {
            // If an inbox is associated with address, we'd use it to create Identity and ignore the nonce.
            // We would need a signature from user's wallet.
            if *associated_inbox_id != inbox_id {
                return Err(IdentityError::NewIdentity("Inbox ID mismatch".to_string()));
            }
            let builder = SignatureRequestBuilder::new(associated_inbox_id.clone());
            let mut signature_request = builder
                .add_association(
                    installation_keys.public_slice().to_vec().into(),
                    member_identifier,
                )
                .build();

            let signature = installation_keys
                .credential_sign::<InstallationKeyContext>(signature_request.signature_text())?;
            signature_request
                .add_signature(
                    UnverifiedSignature::new_installation_key(
                        signature,
                        installation_keys.verifying_key(),
                    ),
                    scw_signature_verifier,
                )
                .await?;

            let identity = Self {
                inbox_id: associated_inbox_id.clone(),
                installation_keys,
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
                .add_association(
                    installation_keys.public_slice().to_vec().into(),
                    member_identifier,
                )
                .build();

            let sig = installation_keys
                .credential_sign::<InstallationKeyContext>(signature_request.signature_text())?;

            signature_request
                .add_signature(
                    UnverifiedSignature::new_installation_key(
                        sig,
                        installation_keys.verifying_key(),
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
                installation_keys,
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
                .add_association(
                    installation_keys.public_slice().to_vec().into(),
                    member_identifier,
                )
                .build();

            let sig = installation_keys
                .credential_sign::<InstallationKeyContext>(signature_request.signature_text())?;
            // We can pre-sign the request with an installation key signature, since we have access to the key
            signature_request
                .add_signature(
                    UnverifiedSignature::new_installation_key(
                        sig,
                        installation_keys.verifying_key(),
                    ),
                    scw_signature_verifier,
                )
                .await?;

            let identity = Self {
                inbox_id: inbox_id.clone(),
                installation_keys,
                credential: create_credential(inbox_id.clone())?,
                signature_request: Some(signature_request),
                is_ready: AtomicBool::new(false),
            };

            Ok(identity)
        }
    }

    pub fn inbox_id(&self) -> InboxIdRef<'_> {
        &self.inbox_id
    }

    pub fn sequence_id(&self, conn: &DbConnection) -> Result<i64, StorageError> {
        conn.get_latest_sequence_id_for_inbox(self.inbox_id.as_str())
    }

    pub fn is_ready(&self) -> bool {
        self.is_ready.load(Ordering::SeqCst)
    }

    pub(crate) fn set_ready(&self) {
        self.is_ready.store(true, Ordering::SeqCst)
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
    pub(crate) fn sign_identity_update<Text: AsRef<str>>(
        &self,
        text: Text,
    ) -> Result<Vec<u8>, IdentityError> {
        self.installation_keys
            .credential_sign::<InstallationKeyContext>(text)
            .map_err(Into::into)
    }

    pub fn sign_with_public_context(
        &self,
        text: impl AsRef<str>,
    ) -> Result<Vec<u8>, IdentityError> {
        self.installation_keys
            .credential_sign::<PublicContext>(text)
            .map_err(Into::into)
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
                    signature_key: self.installation_keys.public_slice().into(),
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

        self.rotate_and_upload_key_package(provider, api_client)
            .await?;
        Ok(StoredIdentity::try_from(self)?.store(provider.conn_ref())?)
    }

    pub(crate) async fn rotate_and_upload_key_package<ApiClient: XmtpApi>(
        &self,
        provider: &XmtpOpenMlsProvider,
        api_client: &ApiClientWrapper<ApiClient>,
    ) -> Result<(), IdentityError> {
        let conn = provider.conn_ref();

        let kp = self.new_key_package(provider)?;
        let kp_bytes = kp.tls_serialize_detached()?;
        let hash_ref = serialize_key_package_hash_ref(&kp, provider)?;
        let history_id = conn.store_key_package_history_entry(hash_ref.clone())?.id;

        match api_client.upload_key_package(kp_bytes, true).await {
            Ok(()) => {
                // Successfully uploaded. Delete previous KPs
                let old_id = history_id - 1;
                provider.transaction(|provider| {
                    let old_key_packages = provider
                        .conn_ref()
                        .find_key_package_history_entries_before_id(old_id)?;
                    for kp in old_key_packages {
                        self.delete_key_package(provider, kp.key_package_hash_ref)?;
                    }
                    conn.delete_key_package_history_entries_before_id(old_id)?;

                    Ok::<_, IdentityError>(())
                })?;

                Ok(())
            }
            Err(err) => {
                // Did not upload. Delete the newly created KP.
                self.delete_key_package(provider, hash_ref)?;
                conn.delete_key_package_entry_with_id(history_id)?;

                Err(IdentityError::ApiClient(err))
            }
        }
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
