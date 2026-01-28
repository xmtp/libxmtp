use crate::groups::mls_ext::{
    WelcomePointersExtension, WrapperAlgorithm, WrapperEncryptionExtension,
};
use crate::identity_updates::{get_association_state_with_verifier, load_identity_updates};
use crate::worker::NeedsDbReconnect;
use crate::{XmtpApi, verified_key_package_v2::KeyPackageVerificationError};
use derive_builder::Builder;
use openmls::prelude::HpkeKeyPair;
use openmls::prelude::hash_ref::HashReference;
use openmls::{
    credentials::{BasicCredential, CredentialWithKey, errors::BasicCredentialError},
    extensions::{
        ApplicationIdExtension, Extension, ExtensionType, Extensions, LastResortExtension,
        UnknownExtension,
    },
    key_packages::KeyPackage,
    messages::proposals::ProposalType,
    prelude::{Capabilities, Credential as OpenMlsCredential, tls_codec::Serialize},
};
use openmls_libcrux_crypto::Provider as LibcruxProvider;
use openmls_traits::{
    OpenMlsProvider, crypto::OpenMlsCrypto, random::OpenMlsRand, types::CryptoError,
};
use prost::Message;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use tls_codec::SecretVLBytes;
use tracing::debug;
use tracing::info;
use xmtp_api::ApiClientWrapper;
use xmtp_common::ErrorCode;
use xmtp_common::time::now_ns;
use xmtp_common::{RetryableError, retryable};
use xmtp_configuration::{
    CIPHERSUITE, CREATE_PQ_KEY_PACKAGE_EXTENSION, GROUP_MEMBERSHIP_EXTENSION_ID,
    GROUP_PERMISSIONS_EXTENSION_ID, KEY_PACKAGE_ROTATION_INTERVAL_NS, MAX_INSTALLATIONS_PER_INBOX,
    MUTABLE_METADATA_EXTENSION_ID, PROPOSAL_SUPPORT_EXTENSION_ID,
    WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID, WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
};
use xmtp_cryptography::configuration::POST_QUANTUM_CIPHERSUITE;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential};
use xmtp_db::db_connection::DbConnection;
use xmtp_db::identity::StoredIdentity;
use xmtp_db::sql_key_store::{
    KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY, SqlKeyStoreError,
};
use xmtp_db::{ConnectionExt, MlsProviderExt};
use xmtp_db::{Fetch, StorageError, Store};
use xmtp_db::{XmtpOpenMlsProviderRef, prelude::*};
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_id::associations::{AssociationError, Identifier, InstallationKeyContext, PublicContext};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::{
    InboxId, InboxIdRef,
    associations::{
        MemberIdentifier,
        builder::{SignatureRequest, SignatureRequestBuilder, SignatureRequestError},
        sign_with_legacy_key,
    },
};
use xmtp_proto::types::InstallationId;
use xmtp_proto::xmtp::identity::MlsCredential;

/**
 * The identity strategy determines how the [`ClientBuilder`](crate::builder::ClientBuilder) constructs an identity on startup.
 *
 * [`IdentityStrategy::CreateIfNotFound`] will attempt to create a new identity if one isn't found in the store.
 * This is the default behavior.
 *
 * [`IdentityStrategy::CachedOnly`] will attempt to get an identity from the store. If not found, it will
 * return an error. This is useful if you don't want to create a new identity on startup because the caller
 * does not have access to a signer.
 *
 * `IdentityStrategy::ExternalIdentity` allows you to provide an already-constructed identity to the
 * client. This is useful for testing and not expected to be used in production.
 */
#[derive(Debug, Clone)]
pub enum IdentityStrategy {
    /// Tries to get an identity from the disk store. If not found, getting one from backend.
    CreateIfNotFound {
        inbox_id: InboxId,
        identifier: Identifier,
        nonce: u64,
        legacy_signed_private_key: Option<Vec<u8>>,
    },
    /// Identity that is already in the disk store
    CachedOnly,
    /// An already-built Identity for testing purposes
    #[cfg(any(test, feature = "test-utils"))]
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
        identifier: Identifier,
        nonce: u64,
        legacy_signed_private_key: Option<Vec<u8>>,
    ) -> Self {
        Self::CreateIfNotFound {
            inbox_id,
            identifier,
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
    pub(crate) async fn initialize_identity<ApiClient: XmtpApi, S: XmtpMlsStorageProvider>(
        self,
        api_client: &ApiClientWrapper<ApiClient>,
        mls_storage: &S,
        scw_signature_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<Identity, IdentityError> {
        use IdentityStrategy::*;

        info!("Initializing identity");
        let stored_identity: Option<Identity> = mls_storage
            .db()
            .fetch(&())?
            .map(|i: StoredIdentity| i.try_into())
            .transpose()?;

        debug!("identity strategy: {self:?}, identity in store: {stored_identity:?}");
        match self {
            CachedOnly => stored_identity.ok_or(IdentityError::RequiredIdentityNotFound),
            CreateIfNotFound {
                inbox_id,
                identifier,
                nonce,
                legacy_signed_private_key,
            } => {
                if let Some(stored_identity) = stored_identity {
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
                } else {
                    Identity::new(
                        inbox_id,
                        identifier,
                        nonce,
                        legacy_signed_private_key,
                        api_client,
                        mls_storage,
                        scw_signature_verifier,
                    )
                    .await
                }
            }
            #[cfg(any(test, feature = "test-utils"))]
            ExternalIdentity(identity) => Ok(identity),
        }
    }
}

#[derive(Debug, Error, ErrorCode)]
pub enum IdentityError {
    #[error(transparent)]
    CredentialSerialization(#[from] prost::EncodeError),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error("installation not found: {0}")]
    InstallationIdNotFound(String),
    #[error(transparent)]
    #[error_code(inherit)]
    SignatureRequestBuilder(#[from] SignatureRequestError),
    #[error(transparent)]
    #[error_code(inherit)]
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
    #[error_code(inherit)]
    StorageError(#[from] xmtp_db::StorageError),
    #[error(transparent)]
    #[error_code(inherit)]
    OpenMlsStorageError(#[from] SqlKeyStoreError),
    #[error(transparent)]
    KeyPackageGenerationError(#[from] openmls::key_packages::errors::KeyPackageNewError),
    #[error(transparent)]
    #[error_code(inherit)]
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
    #[error_code(inherit)]
    Association(#[from] AssociationError),
    #[error(transparent)]
    Signer(#[from] xmtp_cryptography::SignerError),
    #[error(transparent)]
    #[error_code(inherit)]
    ApiClient(#[from] xmtp_api::ApiError),
    #[error(transparent)]
    #[error_code(inherit)]
    AddressValidation(#[from] IdentifierValidationError),
    #[error(transparent)]
    #[error_code(inherit)]
    Db(#[from] xmtp_db::ConnectionError),
    #[error(
        "Cannot register a new installation because the InboxID {inbox_id} has already registered {count}/{max} installations. Please revoke existing installations first."
    )]
    TooManyInstallations {
        inbox_id: String,
        count: usize,
        max: usize,
    },
    #[error(transparent)]
    #[error_code(inherit)]
    GeneratePostQuantumKey(#[from] GeneratePostQuantumKeyError),
    #[error(transparent)]
    InvalidExtension(#[from] openmls::prelude::InvalidExtensionError),
    #[error("Missing post quantum public key")]
    MissingPostQuantumPublicKey,
    #[error("Bincode serialization error")]
    Bincode,
    #[error(transparent)]
    UninitializedField(#[from] derive_builder::UninitializedFieldError),
}

impl NeedsDbReconnect for IdentityError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::StorageError(s) => s.db_needs_connection(),
            _ => false,
        }
    }
}

impl RetryableError for IdentityError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::ApiClient(err) => retryable!(err),
            Self::StorageError(err) => retryable!(err),
            Self::OpenMlsStorageError(err) => retryable!(err),
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

impl TryFrom<&Identity> for StoredIdentity {
    type Error = StorageError;

    fn try_from(identity: &Identity) -> Result<Self, Self::Error> {
        StoredIdentity::builder()
            .inbox_id(identity.inbox_id.clone())
            .installation_keys(xmtp_db::db_serialize(&identity.installation_keys)?)
            .credential_bytes(xmtp_db::db_serialize(&identity.credential())?)
            .next_key_package_rotation_ns(now_ns() + KEY_PACKAGE_ROTATION_INTERVAL_NS)
            .build()
    }
}

impl TryFrom<StoredIdentity> for Identity {
    type Error = StorageError;

    fn try_from(identity: StoredIdentity) -> Result<Self, Self::Error> {
        Ok(Identity {
            inbox_id: identity.inbox_id.clone(),
            installation_keys: xmtp_db::db_deserialize(&identity.installation_keys)?,
            credential: xmtp_db::db_deserialize(&identity.credential_bytes)?,
            signature_request: None,
            is_ready: AtomicBool::new(true),
        })
    }
}

pub(crate) struct NewKeyPackageResult {
    pub(crate) key_package: KeyPackage,
    pub(crate) pq_pub_key: Option<Vec<u8>>,
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
    pub(crate) async fn new<ApiClient: XmtpApi, S: XmtpMlsStorageProvider>(
        inbox_id: InboxId,
        identifier: Identifier,
        nonce: u64,
        legacy_signed_private_key: Option<Vec<u8>>,
        api_client: &ApiClientWrapper<ApiClient>,
        mls_storage: &S,
        scw_signature_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<Self, IdentityError> {
        // check if address is already associated with an inbox_id
        let inbox_ids = api_client
            .get_inbox_ids(vec![identifier.clone().into()])
            .await?;
        let associated_inbox_id = inbox_ids.get(&(&identifier).into());
        let installation_keys = XmtpInstallationCredential::new();

        if let Some(associated_inbox_id) = associated_inbox_id {
            // If an inbox is associated with address, we'd use it to create Identity and ignore the nonce.
            // We would need a signature from user's wallet.
            if *associated_inbox_id != inbox_id {
                return Err(IdentityError::NewIdentity("Inbox ID mismatch".to_string()));
            }

            // get sequence_id from identity updates and loaded into the DB
            load_identity_updates(
                api_client,
                &mls_storage.db(),
                &[associated_inbox_id.as_str()],
            )
            .await
            .map_err(|e| {
                IdentityError::NewIdentity(format!("Failed to load identity updates: {e}"))
            })?;

            let state = get_association_state_with_verifier(
                &mls_storage.db(),
                &inbox_id,
                None,
                &scw_signature_verifier,
            )
            .await
            .map_err(|err| {
                IdentityError::NewIdentity(format!("Error resolving identity state: {}", err))
            })?;

            let current_installation_count = state.installation_ids().len();
            if current_installation_count >= MAX_INSTALLATIONS_PER_INBOX {
                return Err(IdentityError::TooManyInstallations {
                    inbox_id: associated_inbox_id.clone(),
                    count: current_installation_count,
                    max: MAX_INSTALLATIONS_PER_INBOX,
                });
            }

            let builder = SignatureRequestBuilder::new(associated_inbox_id.clone());
            let mut signature_request = builder
                .add_association(
                    MemberIdentifier::installation(installation_keys.public_slice().to_vec()),
                    identifier.clone().into(),
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
            let generated_inbox_id = identifier.inbox_id(nonce)?;
            if inbox_id != generated_inbox_id {
                return Err(IdentityError::NewIdentity(
                    "Inbox ID doesn't match nonce & address".to_string(),
                ));
            }
            let mut builder = SignatureRequestBuilder::new(inbox_id.clone());
            builder = builder.create_inbox(identifier.clone(), nonce);
            let mut signature_request = builder
                .add_association(
                    MemberIdentifier::installation(installation_keys.public_slice().to_vec()),
                    identifier.clone().into(),
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
                    UnverifiedSignature::LegacyDelegated(sign_with_legacy_key(
                        signature_request.signature_text(),
                        legacy_signed_private_key,
                    )?),
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

            identity.register(api_client, mls_storage).await?;

            let identity_update = signature_request.build_identity_update()?;
            api_client.publish_identity_update(identity_update).await?;

            Ok(identity)
        } else {
            let generated_inbox_id = identifier.inbox_id(nonce)?;
            if inbox_id != generated_inbox_id {
                return Err(IdentityError::NewIdentity(
                    "Inbox ID doesn't match nonce & address".to_string(),
                ));
            }
            let mut builder = SignatureRequestBuilder::new(inbox_id.clone());
            builder = builder.create_inbox(identifier.clone(), nonce);

            let mut signature_request = builder
                .add_association(
                    MemberIdentifier::installation(installation_keys.public_slice().to_vec()),
                    identifier.clone().into(),
                )
                .build();

            // We can pre-sign the request with an installation key signature, since we have access to the key
            let sig = installation_keys
                .credential_sign::<InstallationKeyContext>(signature_request.signature_text())?;
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

    pub fn installation_id(&self) -> InstallationId {
        (*self.installation_keys.public_bytes()).into()
    }

    pub fn sequence_id<C>(&self, conn: &DbConnection<C>) -> Result<i64, xmtp_db::ConnectionError>
    where
        C: ConnectionExt,
    {
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
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn new_key_package(
        &self,
        provider: &impl MlsProviderExt,
        include_post_quantum: bool,
    ) -> Result<NewKeyPackageResult, IdentityError> {
        XmtpKeyPackage::builder()
            .inbox_id(self.inbox_id())
            .credential(self.credential())
            .installation_keys(self.installation_keys.clone())
            .build(provider, include_post_quantum)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) async fn register<ApiClient: XmtpApi, S: XmtpMlsStorageProvider>(
        &self,
        api_client: &ApiClientWrapper<ApiClient>,
        mls_storage: &S,
    ) -> Result<(), IdentityError> {
        let stored_identity: Option<StoredIdentity> = mls_storage.db().fetch(&())?;
        if stored_identity.is_some() {
            info!("Identity already registered. skipping key package publishing");
            return Ok(());
        }

        self.rotate_and_upload_key_package(
            api_client,
            mls_storage,
            CREATE_PQ_KEY_PACKAGE_EXTENSION,
        )
        .await?;
        Ok(StoredIdentity::try_from(self)?.store(&mls_storage.db())?)
    }

    /// If no key rotation is scheduled, queue it to occur in the next 5 seconds.
    pub(crate) async fn queue_key_rotation(
        &self,
        conn: &impl DbQuery,
    ) -> Result<(), IdentityError> {
        conn.queue_key_package_rotation()?;
        tracing::info!("Last key package not ready for rotation, queued for rotation");
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) async fn rotate_and_upload_key_package<
        ApiClient: XmtpApi,
        S: XmtpMlsStorageProvider,
    >(
        &self,
        api_client: &ApiClientWrapper<ApiClient>,
        mls_storage: &S,
        include_post_quantum: bool,
    ) -> Result<(), IdentityError> {
        tracing::info!("Start rotating keys and uploading the new key package");

        // Generate and store key package locally
        let (kp_bytes, history_id) =
            self.generate_and_store_key_package(mls_storage, include_post_quantum)?;

        // Upload to network
        match api_client.upload_key_package(kp_bytes, true).await {
            Ok(()) => {
                // Successfully uploaded. Delete previous KPs
                let provider = XmtpOpenMlsProviderRef::new(mls_storage);
                provider.storage().transaction(|conn| {
                    let storage = conn.key_store();
                    storage
                        .db()
                        .mark_key_package_before_id_to_be_deleted(history_id)?;
                    Ok::<(), StorageError>(())
                })?;
                mls_storage
                    .db()
                    .reset_key_package_rotation_queue(KEY_PACKAGE_ROTATION_INTERVAL_NS)?;
                Ok(())
            }
            Err(err) => Err(IdentityError::ApiClient(err)),
        }
    }

    /// Generate and store key package locally (not uploaded to network).
    /// Returns serialized bytes and history ID for later upload/cleanup.
    /// Prevents orphaned key packages if signature validation fails.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn generate_and_store_key_package<S: XmtpMlsStorageProvider>(
        &self,
        mls_storage: &S,
        include_post_quantum: bool,
    ) -> Result<(Vec<u8>, i32), IdentityError> {
        let provider = XmtpOpenMlsProviderRef::new(mls_storage);
        let NewKeyPackageResult {
            key_package: kp,
            pq_pub_key,
        } = self.new_key_package(&provider, include_post_quantum)?;

        let hash_ref = serialize_key_package_hash_ref(&kp, &provider)?;
        let history_id = provider
            .storage()
            .db()
            .store_key_package_history_entry(hash_ref, pq_pub_key)?
            .id;

        let kp_bytes = kp.tls_serialize_detached()?;

        Ok((kp_bytes, history_id))
    }
}

#[cfg(any(test, feature = "test-utils"))]
tokio::task_local! {
    pub static ENABLE_WELCOME_POINTERS: bool;
}

#[derive(Builder, Debug)]
#[builder(build_fn(error = "IdentityError", name = "inner_build", private))]
pub struct XmtpKeyPackage {
    #[builder(setter(into))]
    inbox_id: String,
    #[builder(setter(into))]
    credential: OpenMlsCredential,
    #[builder(setter(into))]
    installation_keys: XmtpInstallationCredential,
}

impl XmtpKeyPackage {
    pub(crate) fn builder() -> XmtpKeyPackageBuilder {
        XmtpKeyPackageBuilder::default()
    }
}

impl XmtpKeyPackageBuilder {
    pub(crate) fn build(
        &mut self,
        provider: &impl MlsProviderExt,
        include_post_quantum: bool,
    ) -> Result<NewKeyPackageResult, IdentityError> {
        let this = self.inner_build()?;
        let last_resort = Extension::LastResort(LastResortExtension::default());
        let welcome_pointee_encryption_aead_types =
            WelcomePointersExtension::available_types().try_into()?;
        let mut extensions = vec![last_resort, welcome_pointee_encryption_aead_types];
        #[cfg(any(test, feature = "test-utils"))]
        {
            if !ENABLE_WELCOME_POINTERS.try_with(|v| *v).unwrap_or(true) {
                let extension = extensions
                    .pop()
                    .expect("Welcome pointers extension is always present");
                assert_eq!(
                    extension.extension_type(),
                    ExtensionType::Unknown(WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID)
                );
            }
        }
        let mut post_quantum_keypair = None;
        if include_post_quantum {
            let keypair = generate_post_quantum_key()?;
            extensions.push(build_post_quantum_public_key_extension(&keypair.public)?);
            post_quantum_keypair = Some(keypair);
        }
        let key_package_extensions = Extensions::from_vec(extensions)?;

        let application_id =
            Extension::ApplicationId(ApplicationIdExtension::new(this.inbox_id.as_bytes()));
        // Version 1 of proposal support - advertises that this installation can receive proposals
        let proposal_support =
            Extension::Unknown(PROPOSAL_SUPPORT_EXTENSION_ID, UnknownExtension(vec![1]));
        let leaf_node_extensions = Extensions::from_vec(vec![application_id, proposal_support])?;

        let capabilities = Capabilities::new(
            None,
            Some(&[CIPHERSUITE]),
            Some(&[
                ExtensionType::LastResort,
                ExtensionType::ApplicationId,
                ExtensionType::Unknown(GROUP_PERMISSIONS_EXTENSION_ID),
                ExtensionType::Unknown(MUTABLE_METADATA_EXTENSION_ID),
                ExtensionType::Unknown(GROUP_MEMBERSHIP_EXTENSION_ID),
                ExtensionType::Unknown(WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID),
                ExtensionType::Unknown(WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID),
                ExtensionType::Unknown(PROPOSAL_SUPPORT_EXTENSION_ID),
                ExtensionType::ImmutableMetadata,
            ]),
            Some(&[ProposalType::GroupContextExtensions]),
            None,
        );

        let kp_builder = KeyPackage::builder()
            .leaf_node_capabilities(capabilities)
            .leaf_node_extensions(leaf_node_extensions)
            .key_package_extensions(key_package_extensions);

        let kp_builder = {
            #[cfg(any(test, feature = "test-utils"))]
            {
                use crate::utils::test_mocks_helpers::maybe_mock_package_lifetime;
                let life_time = maybe_mock_package_lifetime();
                kp_builder.key_package_lifetime(life_time)
            }
            #[cfg(not(any(test, feature = "test-utils")))]
            {
                kp_builder
            }
        };

        let kp = kp_builder.build(
            CIPHERSUITE,
            provider,
            &this.installation_keys,
            CredentialWithKey {
                credential: this.credential,
                signature_key: this.installation_keys.public_slice().into(),
            },
        )?;

        store_key_package_references(provider, kp.key_package(), &post_quantum_keypair)?;
        Ok(NewKeyPackageResult {
            key_package: kp.key_package().clone(),
            pq_pub_key: post_quantum_keypair.map(|kp| kp.public),
        })
    }
}

/// Serialize the key package hash ref to a bincode friendly format that is compatible with `read` in the KeyStore
pub(crate) fn serialize_key_package_hash_ref(
    kp: &KeyPackage,
    provider: &impl MlsProviderExt,
) -> Result<Vec<u8>, IdentityError> {
    let key_package_hash_ref = kp
        .hash_ref(provider.crypto())
        .map_err(|_| IdentityError::UninitializedIdentity)?;
    let serialized = bincode::serialize(&key_package_hash_ref)
        .map_err(|_| IdentityError::UninitializedIdentity)?;

    Ok(serialized)
}

// Takes a post quantum public key and returns the key used to store it in the key package references table
pub(crate) fn pq_key_package_references_key(
    raw_pub_key: &Vec<u8>,
) -> Result<Vec<u8>, IdentityError> {
    Ok(raw_pub_key.tls_serialize_detached()?)
}

pub(crate) fn deserialize_key_package_hash_ref(
    hash_ref: &[u8],
) -> Result<HashReference, IdentityError> {
    let key_package_hash_ref: HashReference =
        bincode::deserialize(hash_ref).map_err(|_| IdentityError::UninitializedIdentity)?;

    Ok(key_package_hash_ref)
}

pub(crate) fn create_credential(
    inbox_id: impl AsRef<str>,
) -> Result<OpenMlsCredential, IdentityError> {
    let inbox_id = inbox_id.as_ref().to_string();
    let cred = MlsCredential { inbox_id };
    let mut credential_bytes = Vec::new();
    let _ = cred.encode(&mut credential_bytes);

    Ok(BasicCredential::new(credential_bytes).into())
}

pub fn parse_credential(credential_bytes: &[u8]) -> Result<InboxId, IdentityError> {
    let cred = MlsCredential::decode(credential_bytes)?;
    Ok(cred.inbox_id)
}

pub fn build_post_quantum_public_key_extension(
    public_key: &[u8],
) -> Result<Extension, IdentityError> {
    let ext =
        WrapperEncryptionExtension::new(WrapperAlgorithm::XWingMLKEM768Draft6, public_key.to_vec());

    Ok(ext.try_into()?)
}

/// Error type for generating a post quantum key pair
#[derive(Debug, Error)]
pub enum GeneratePostQuantumKeyError {
    #[error(transparent)]
    Crypto(#[from] openmls_traits::types::CryptoError),
    #[error(transparent)]
    Rand(#[from] openmls_libcrux_crypto::RandError),
}

impl xmtp_common::ErrorCode for GeneratePostQuantumKeyError {
    fn error_code(&self) -> &'static str {
        match self {
            Self::Crypto(_) => "GeneratePostQuantumKeyError::Crypto",
            Self::Rand(_) => "GeneratePostQuantumKeyError::Rand",
        }
    }
}

/// Generate a new key pair using our post quantum ciphersuite
pub(crate) fn generate_post_quantum_key() -> Result<HpkeKeyPair, GeneratePostQuantumKeyError> {
    let provider = LibcruxProvider::default();
    let rand = provider.rand();

    let ikm: SecretVLBytes = rand
        .random_vec(POST_QUANTUM_CIPHERSUITE.hash_length())?
        .into();

    Ok(provider
        .crypto()
        .derive_hpke_keypair(POST_QUANTUM_CIPHERSUITE.hpke_config(), ikm.as_slice())?)
}

// Store the hash reference, keyed with both the public init key and the post quantum init key.
// This is needed to get to the private key when decrypting welcome messages.
// Both the Curve25519 and the Post Quantum keys hold a hash reference to the key package.
// If a post quantum key is present, we also have a pointer from the key package hash ref -> the post quantum private key.
pub(crate) fn store_key_package_references(
    provider: &impl MlsProviderExt,
    kp: &KeyPackage,
    // The post quantum init key for the key package used for Post Quantum Welcome Wrapper encryption
    post_quantum_keypair: &Option<HpkeKeyPair>,
) -> Result<(), IdentityError> {
    // For dumb legacy reasons that are probably my fault, we keep the key package references
    // keyed by the TLS serialized public init key instead of the slice version.
    let public_init_key = kp.hpke_init_key().tls_serialize_detached()?;

    let hash_ref = serialize_key_package_hash_ref(kp, provider)?;
    let storage = provider.key_store();
    // Write the normal init key to the key package references
    storage.write(KEY_PACKAGE_REFERENCES, &public_init_key, &hash_ref)?;

    if let Some(post_quantum_keypair) = post_quantum_keypair {
        let post_quantum_public_key = pq_key_package_references_key(&post_quantum_keypair.public)?;
        // We need to store this in a bincode friendly format so that `read` will work later.
        // TODO:(nm) review whether this breaks the Zeroize guarantees
        let post_quantum_private_key = bincode::serialize(&post_quantum_keypair.private.to_vec())
            .map_err(|_| IdentityError::Bincode)?;

        // Write the post quantum wrapper encryption public key to the key package references
        storage.write(KEY_PACKAGE_REFERENCES, &post_quantum_public_key, &hash_ref)?;

        storage.write(
            KEY_PACKAGE_WRAPPER_PRIVATE_KEY,
            &hash_ref,
            &post_quantum_private_key,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::context::XmtpSharedContext;
    use crate::groups::mls_ext::WrapperAlgorithm;
    use crate::{
        builder::ClientBuilder,
        groups::key_package_cleaner_worker::KeyPackagesCleanerWorker,
        identity::{pq_key_package_references_key, serialize_key_package_hash_ref},
        utils::FullXmtpClient,
        verified_key_package_v2::VerifiedKeyPackageV2,
    };
    use openmls::prelude::{KeyPackageBundle, KeyPackageRef};
    use openmls_traits::{OpenMlsProvider, storage::StorageProvider};
    use tls_codec::Serialize;
    use xmtp_api_d14n::protocol::XmtpQuery;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::XmtpMlsStorageProvider;
    use xmtp_db::XmtpOpenMlsProviderRef;
    use xmtp_db::{
        MlsProviderExt,
        group::{ConversationType, GroupQueryArgs},
        sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY},
    };
    use xmtp_mls_common::group::DMMetadataOptions;
    use xmtp_proto::types::TopicKind;

    async fn get_key_package_from_network(client: &FullXmtpClient) -> VerifiedKeyPackageV2 {
        let mut kp_mapping = client
            .get_key_packages_for_installation_ids(vec![client.installation_public_key().to_vec()])
            .await
            .unwrap();

        kp_mapping
            .remove(client.installation_public_key().as_slice())
            .unwrap()
            .unwrap()
    }

    async fn get_latest_welcome(client: &FullXmtpClient) -> xmtp_proto::types::WelcomeMessage {
        let welcomes = client
            .context
            .api()
            .query_at(
                TopicKind::WelcomeMessagesV1.create(client.context.installation_id()),
                None,
            )
            .await
            .unwrap()
            .welcome_messages()
            .unwrap();

        welcomes[0].clone()
    }

    /// Look up the key package hash ref by public init key
    fn get_hash_ref(provider: &impl MlsProviderExt, pub_key: &[u8]) -> Option<KeyPackageRef> {
        provider
            .key_store()
            .read(KEY_PACKAGE_REFERENCES, pub_key)
            .unwrap()
    }

    fn get_pq_private_key(provider: &impl MlsProviderExt, hash_ref: &[u8]) -> Option<Vec<u8>> {
        let val: Option<Vec<u8>> = provider
            .key_store()
            .read::<Vec<u8>>(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, hash_ref)
            .unwrap();

        val
    }

    #[xmtp_common::test]
    async fn ensure_pq_keys_are_deleted() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let storage = client.context.mls_storage();
        let provider = XmtpOpenMlsProviderRef::new(storage);

        // As long as we have `config::CREATE_PQ_KEY_PACKAGE_EXTENSION` set to false, we need to do this step to force a PQ key package to be created
        let api_client = client.context.api();
        client
            .identity()
            .rotate_and_upload_key_package(api_client, storage, true)
            .await
            .unwrap();

        // Get the key package back from the network
        let starting_key_package = get_key_package_from_network(&client).await;
        let starting_init_key = starting_key_package
            .inner
            .hpke_init_key()
            .tls_serialize_detached()
            .unwrap();

        // Make sure we can find the init key
        let init_key_hash_ref = get_hash_ref(&provider, &starting_init_key);
        assert!(init_key_hash_ref.is_some());

        // Make sure we can find the post quantum public key
        let pq_public_key = starting_key_package.wrapper_encryption().unwrap();
        assert!(pq_public_key.is_some());

        let pq_public_key_bytes = pq_public_key.unwrap().pub_key_bytes;
        let pq_hash_ref = get_hash_ref(
            &provider,
            &pq_key_package_references_key(&pq_public_key_bytes).unwrap(),
        );
        assert!(pq_hash_ref.is_some());
        let pq_hash_ref_inner = pq_hash_ref.unwrap();

        // Make sure we can find the key package based on the post quantum public key
        let key_package_bundle: KeyPackageBundle = provider
            .storage()
            .key_package(&pq_hash_ref_inner)
            .unwrap()
            .unwrap();

        // Make sure we can find the private key based on the init key
        let serialized_hash_ref = bincode::serialize(&init_key_hash_ref.unwrap()).unwrap();
        let pq_private_key = get_pq_private_key(&provider, &serialized_hash_ref);
        assert!(pq_private_key.is_some());

        // Now rotate the key package
        client.rotate_and_upload_key_package().await.unwrap();

        // Force deletion of the key package, even though it hasn't expired yet
        let cleaner = KeyPackagesCleanerWorker::new(client.context.clone());
        let serialized_key_package_hash_ref =
            serialize_key_package_hash_ref(key_package_bundle.key_package(), &provider).unwrap();
        cleaner
            .delete_key_package(
                serialized_key_package_hash_ref,
                Some(pq_public_key_bytes.clone()),
            )
            .unwrap();

        // Now test to see if the private keys are deleted by doing the same steps as above
        let pq_hash_ref = get_hash_ref(
            &provider,
            &pq_key_package_references_key(&pq_public_key_bytes).unwrap(),
        );
        assert!(pq_hash_ref.is_none());

        let pq_private_key = get_pq_private_key(&provider, &serialized_hash_ref);
        assert!(pq_private_key.is_none());

        let key_package_from_db: Option<KeyPackageBundle> =
            provider.storage().key_package(&pq_hash_ref_inner).unwrap();
        assert!(key_package_from_db.is_none());
    }

    #[test]
    fn test_generate_post_quantum_key_error_codes() {
        use super::GeneratePostQuantumKeyError;
        use openmls_traits::types::CryptoError;
        use xmtp_common::ErrorCode;

        // Test Crypto variant
        let crypto_err = GeneratePostQuantumKeyError::Crypto(CryptoError::CryptoLibraryError);
        assert_eq!(
            crypto_err.error_code(),
            "GeneratePostQuantumKeyError::Crypto"
        );
    }

    #[test]
    fn test_identity_error_codes() {
        use super::IdentityError;
        use xmtp_common::ErrorCode;

        // Test simple variants
        let err = IdentityError::LegacyKeyReuse;
        assert_eq!(err.error_code(), "IdentityError::LegacyKeyReuse");

        let err = IdentityError::UninitializedIdentity;
        assert_eq!(err.error_code(), "IdentityError::UninitializedIdentity");

        let err = IdentityError::LegacyKeyMismatch;
        assert_eq!(err.error_code(), "IdentityError::LegacyKeyMismatch");

        let err = IdentityError::RequiredIdentityNotFound;
        assert_eq!(err.error_code(), "IdentityError::RequiredIdentityNotFound");

        let err = IdentityError::Bincode;
        assert_eq!(err.error_code(), "IdentityError::Bincode");

        let err = IdentityError::MissingPostQuantumPublicKey;
        assert_eq!(
            err.error_code(),
            "IdentityError::MissingPostQuantumPublicKey"
        );

        // Test variants with data
        let err = IdentityError::InstallationIdNotFound("test".to_string());
        assert_eq!(err.error_code(), "IdentityError::InstallationIdNotFound");

        let err = IdentityError::InstallationKey("test".to_string());
        assert_eq!(err.error_code(), "IdentityError::InstallationKey");

        let err = IdentityError::NewIdentity("test".to_string());
        assert_eq!(err.error_code(), "IdentityError::NewIdentity");

        let err = IdentityError::TooManyInstallations {
            inbox_id: "test".to_string(),
            count: 10,
            max: 5,
        };
        assert_eq!(err.error_code(), "IdentityError::TooManyInstallations");

        let err = IdentityError::InboxIdMismatch {
            id: "id1".to_string(),
            stored: "id2".to_string(),
        };
        assert_eq!(err.error_code(), "IdentityError::InboxIdMismatch");

        let err = IdentityError::NoAssociatedInboxId("addr".to_string());
        assert_eq!(err.error_code(), "IdentityError::NoAssociatedInboxId");
    }

    #[test]
    fn test_identity_error_inherited_codes() {
        use super::IdentityError;
        use xmtp_common::ErrorCode;
        use xmtp_db::{NotFound, StorageError};

        // Test inherited error codes
        let storage_err = StorageError::NotFound(NotFound::MessageById(vec![1, 2, 3]));
        let err = IdentityError::StorageError(storage_err);
        assert_eq!(err.error_code(), "StorageError::NotFound");
    }

    #[xmtp_common::test]
    async fn post_quantum_interop() {
        for [amal_has_pq, bola_has_pq] in
            [[true, false], [false, true], [true, true], [false, false]]
        {
            let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
            let amal_api = amal.context.api();
            let amal_mls = amal.context.mls_storage();

            let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
            let bola_api = bola.context.api();
            let bola_mls = bola.context.mls_storage();

            // Give amal a post quantum key package and bola a legacy key package
            amal.identity()
                .rotate_and_upload_key_package(amal_api, amal_mls, amal_has_pq)
                .await
                .unwrap();
            bola.identity()
                .rotate_and_upload_key_package(bola_api, bola_mls, bola_has_pq)
                .await
                .unwrap();

            // Create a DM from Amal -> Bola
            // This should use Bola's XWingMLKEM512 key package
            amal.find_or_create_dm(
                bola.inbox_id().to_string(),
                Some(DMMetadataOptions::default()),
            )
            .await
            .unwrap();

            // Sync both clients
            amal.sync_welcomes().await.unwrap();
            bola.sync_welcomes().await.unwrap();

            // Get the DMs from the clients
            let query_args = GroupQueryArgs {
                conversation_type: Some(ConversationType::Dm),
                ..GroupQueryArgs::default()
            };

            let amal_convos = amal.list_conversations(query_args.clone()).unwrap();
            let bola_convos = bola.list_conversations(query_args).unwrap();

            assert_eq!(amal_convos.len(), 1);
            assert_eq!(bola_convos.len(), 1);

            let amal_key_package = get_key_package_from_network(&amal).await;
            let bola_key_package = get_key_package_from_network(&bola).await;

            assert_eq!(
                amal_key_package.wrapper_encryption().unwrap().is_some(),
                amal_has_pq
            );
            assert_eq!(
                bola_key_package.wrapper_encryption().unwrap().is_some(),
                bola_has_pq
            );

            // Get the welcome messages from the network
            let bola_welcome = get_latest_welcome(&bola).await;

            // Make sure the wrapper algorithms were set correctly in the Welcome messages
            let pq_algorithm = WrapperAlgorithm::XWingMLKEM768Draft6;
            let traditional_algorithm = WrapperAlgorithm::Curve25519;
            let bola_wrapper = bola_welcome.as_v1().unwrap().wrapper_algorithm;
            if bola_has_pq {
                assert_eq!(bola_wrapper, pq_algorithm.into());
            } else {
                assert_eq!(bola_wrapper, traditional_algorithm.into());
            }
        }
    }
}
