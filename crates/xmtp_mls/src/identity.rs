pub use xmtp_cryptography::GeneratePostQuantumKeyError;
mod identity_ext;
pub use identity_ext::*;

use crate::XmtpApi;
use crate::worker::NeedsDbReconnect;
use derive_builder::Builder;
use openmls::prelude::HpkeKeyPair;
use openmls::prelude::hash_ref::HashReference;
use openmls::{
    credentials::errors::BasicCredentialError,
    extensions::Extension,
    key_packages::KeyPackage,
    prelude::{
        Credential as OpenMlsCredential,
        tls_codec::{Deserialize as _, Serialize, VLBytes},
    },
};
use openmls_traits::types::CryptoError;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use tracing::debug;
use tracing::info;
use xmtp_api::ApiClientWrapper;
use xmtp_common::ErrorCode;
use xmtp_common::snippet::Snippet;
use xmtp_common::time::now_ns;
use xmtp_common::{RetryableError, retryable};
use xmtp_configuration::KEY_PACKAGE_ROTATION_INTERVAL_NS;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential};
use xmtp_db::db_connection::DbConnection;
use xmtp_db::identity::StoredIdentity;
use xmtp_db::prelude::*;
use xmtp_db::sql_key_store::{
    KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY, SqlKeyStoreError, codec as kv_codec,
};
use xmtp_db::{ConnectionExt, MlsProviderExt};
use xmtp_db::{Fetch, StorageError, StoredSecret};
use xmtp_id::associations::{AssociationError, Identifier, InstallationKeyContext, PublicContext};
use xmtp_id::key_package::KeyPackageVerificationError;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::{
    InboxId, InboxIdRef,
    associations::builder::{SignatureRequest, SignatureRequestError},
};
use xmtp_proto::types::InstallationId;

/**
 * The identity strategy determines how the [`ClientBuilder`](crate::builder::ClientBuilder) constructs an identity on startup.
 *
 * [`IdentityStrategy::CreateIfNotFound`] will attempt to create a new identity if one isn't found in the store.
 * This is the default behavior.
 *
 * [`IdentityStrategy::CreateForIdentifier`] acts as `CreateIfNotFound` for the inbox of an identifier,
 * which the builder asks the backend for only after it has checked the backend's deployment.
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
    },
    /// `CreateIfNotFound` for the inbox the backend associates with the
    /// identifier, or else the inbox its registration at the nonce creates.
    CreateForIdentifier { identifier: Identifier, nonce: u64 },
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
    pub fn new(inbox_id: InboxId, identifier: Identifier, nonce: u64) -> Self {
        Self::CreateIfNotFound {
            inbox_id,
            identifier,
            nonce,
        }
    }

    /// Create an [`IdentityStrategy::CreateForIdentifier`], for a caller that
    /// does not know the identifier's inbox. The build takes the inbox the
    /// backend names, or the one the identifier creates at `nonce`, and fails
    /// with [`IdentityError::IdentifierNotInInbox`] unless the identifier
    /// belongs to it, as
    /// [`ClientBuilder::build`](crate::builder::ClientBuilder::build)
    /// describes. An offline build checks the newest state the database
    /// stores, which can predate a revocation.
    pub fn for_identifier(identifier: Identifier, nonce: u64) -> Self {
        Self::CreateForIdentifier { identifier, nonce }
    }

    /// Replace [`IdentityStrategy::CreateForIdentifier`] with
    /// [`IdentityStrategy::CreateIfNotFound`] for the identifier's inbox. Call
    /// it only after the backend's deployment is checked: the request carries
    /// the identifier.
    pub(crate) async fn with_inbox<ApiClient: XmtpApi>(
        self,
        api_client: &ApiClientWrapper<ApiClient>,
    ) -> Result<Self, IdentityError> {
        let Self::CreateForIdentifier { identifier, nonce } = self else {
            return Ok(self);
        };
        let found = api_client
            .get_inbox_ids(vec![identifier.clone().into()])
            .await?;
        let inbox_id = match found.into_iter().next().flatten() {
            Some(inbox_id) => inbox_id,
            None => identifier.inbox_id(nonce)?,
        };
        Ok(Self::new(inbox_id, identifier, nonce))
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

        let strategy = match &self {
            CreateIfNotFound { .. } => "create_if_not_found",
            CreateForIdentifier { .. } => "create_for_identifier",
            CachedOnly => "cached_only",
            #[cfg(any(test, feature = "test-utils"))]
            ExternalIdentity(_) => "external_identity",
        };
        debug!(
            strategy,
            stored_identity_present = stored_identity.is_some(),
            "identity strategy"
        );
        match self.with_inbox(api_client).await? {
            CachedOnly => stored_identity.ok_or(IdentityError::RequiredIdentityNotFound),
            CreateForIdentifier { .. } => unreachable!("with_inbox replaces the strategy"),
            CreateIfNotFound {
                inbox_id,
                identifier,
                nonce,
            } => {
                if let Some(stored_identity) = stored_identity {
                    tracing::debug!(
                        installation_id =
                            stored_identity.installation_keys.public_bytes().snippet(),
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
                    crate::identity_lifecycle::new_identity(
                        inbox_id,
                        identifier,
                        nonce,
                        api_client,
                        mls_storage,
                        &scw_signature_verifier,
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
    /// Credential serialization error.
    ///
    /// Failed to encode MLS credential. Not retryable.
    #[error(transparent)]
    CredentialSerialization(#[from] prost::EncodeError),
    /// Decode error.
    ///
    /// Protobuf decoding failed. Not retryable.
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    /// Installation not found.
    ///
    /// Installation ID not found in network association state. Not retryable.
    #[error("installation not found: {0}")]
    InstallationIdNotFound(String),
    #[error(transparent)]
    #[error_code(inherit)]
    SignatureRequestBuilder(#[from] SignatureRequestError),
    #[error(transparent)]
    #[error_code(inherit)]
    Signature(#[from] xmtp_id::associations::SignatureError),
    /// Basic credential error.
    ///
    /// MLS basic credential validation failed. Not retryable.
    #[error(transparent)]
    BasicCredential(#[from] BasicCredentialError),
    /// Uninitialized identity.
    ///
    /// Identity not yet initialized. Not retryable.
    #[error("Uninitialized identity")]
    UninitializedIdentity,
    /// Installation key error.
    ///
    /// Problem with installation key. Not retryable.
    #[error("Installation key {0}")]
    InstallationKey(String),
    /// Crypto error.
    ///
    /// Cryptographic operation failed. Not retryable.
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    /// OpenMLS error.
    ///
    /// OpenMLS library error. Not retryable.
    #[error(transparent)]
    OpenMls(#[from] openmls::prelude::Error),
    #[error(transparent)]
    #[error_code(inherit)]
    StorageError(#[from] xmtp_db::StorageError),
    #[error(transparent)]
    #[error_code(inherit)]
    OpenMlsStorageError(#[from] SqlKeyStoreError),
    /// Key package generation error.
    ///
    /// Failed to generate MLS key package. Not retryable.
    #[error(transparent)]
    KeyPackageGenerationError(#[from] openmls::key_packages::errors::KeyPackageNewError),
    #[error(transparent)]
    #[error_code(inherit)]
    KeyPackageVerificationError(#[from] KeyPackageVerificationError),
    /// Inbox ID mismatch.
    ///
    /// Associated InboxID does not match stored value. Not retryable.
    #[error("The InboxID {id}, associated does not match the stored InboxId {stored}.")]
    InboxIdMismatch { id: InboxId, stored: InboxId },
    /// Identifier not in inbox.
    ///
    /// The identifier does not belong to the inbox of the stored identity. Not retryable.
    #[error("The identifier does not belong to the stored InboxId {inbox_id}.")]
    IdentifierNotInInbox { inbox_id: InboxId },
    /// No associated Inbox ID.
    ///
    /// Address has no associated InboxID. Not retryable.
    #[error("The address {0} has no associated InboxID")]
    NoAssociatedInboxId(String),
    /// Required identity not found.
    ///
    /// Identity was not found in cache. Not retryable.
    #[error("Required identity was not found in cache.")]
    RequiredIdentityNotFound,
    /// New identity creation error.
    ///
    /// Error creating a new identity. Not retryable.
    #[error("error creating new identity: {0}")]
    NewIdentity(String),
    #[error(transparent)]
    #[error_code(inherit)]
    Association(#[from] AssociationError),
    /// Signer error.
    ///
    /// Cryptographic signer failed. Not retryable.
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
    /// Too many installations.
    ///
    /// InboxID has reached max installation count. Not retryable.
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
    /// Invalid extension error.
    ///
    /// MLS extension validation failed. Not retryable.
    #[error(transparent)]
    InvalidExtension(#[from] openmls::prelude::InvalidExtensionError),
    /// Missing PQ public key.
    ///
    /// Post-quantum public key not found. Not retryable.
    #[error("Missing post quantum public key")]
    MissingPostQuantumPublicKey,
    /// Uninitialized field.
    ///
    /// Builder field not initialized. Not retryable.
    #[error(transparent)]
    UninitializedField(#[from] derive_builder::UninitializedFieldError),
}

impl From<xmtp_db::diesel::result::Error> for IdentityError {
    fn from(error: xmtp_db::diesel::result::Error) -> Self {
        Self::StorageError(error.into())
    }
}

impl NeedsDbReconnect for IdentityError {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::StorageError(s) => s.db_needs_connection(),
            Self::Db(c) => c.db_needs_connection(),
            // Keystore ops (rotate/delete paths) hit the same pool.
            Self::OpenMlsStorageError(SqlKeyStoreError::Connection(c)) => c.db_needs_connection(),
            Self::OpenMlsStorageError(_)
            | Self::CredentialSerialization(..)
            | Self::Decode(..)
            | Self::InstallationIdNotFound(..)
            | Self::SignatureRequestBuilder(..)
            | Self::Signature(..)
            | Self::BasicCredential(..)
            | Self::UninitializedIdentity
            | Self::InstallationKey(..)
            | Self::Crypto(..)
            | Self::OpenMls(..)
            | Self::KeyPackageGenerationError(..)
            | Self::KeyPackageVerificationError(..)
            | Self::InboxIdMismatch { .. }
            | Self::IdentifierNotInInbox { .. }
            | Self::NoAssociatedInboxId(..)
            | Self::RequiredIdentityNotFound
            | Self::NewIdentity(..)
            | Self::Association(..)
            | Self::Signer(..)
            | Self::ApiClient(..)
            | Self::AddressValidation(..)
            | Self::TooManyInstallations { .. }
            | Self::GeneratePostQuantumKey(..)
            | Self::InvalidExtension(..)
            | Self::MissingPostQuantumPublicKey
            | Self::UninitializedField(..) => false,
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

    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        match self {
            Self::ApiClient(err) => err.rate_limit_delay(),
            _ => None,
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
            .installation_keys(StoredSecret::from(xmtp_db::db_serialize(
                &identity.installation_keys,
            )?))
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
            installation_keys: xmtp_db::db_deserialize(identity.installation_keys.as_slice())?,
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
}

/// Whether key packages built in its scope advertise welcome pointees, with a
/// count of the packages that read it so a test can tell that it ran.
#[cfg(any(test, feature = "test-hooks"))]
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct WelcomePointersHook {
    enabled: bool,
    reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(any(test, feature = "test-hooks"))]
impl WelcomePointersHook {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            reads: Default::default(),
        }
    }

    /// Record a package build that reads this hook, and return its setting.
    pub fn read(&self) -> bool {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.enabled
    }

    /// How many package builds read this hook.
    pub fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}

#[cfg(any(test, feature = "test-hooks"))]
tokio::task_local! {
    pub static ENABLE_WELCOME_POINTERS: WelcomePointersHook;
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
        #[allow(unused_mut)]
        let mut options = xmtp_id::key_package::KeyPackageOptions {
            include_post_quantum,
            ..Default::default()
        };
        #[cfg(any(test, feature = "test-hooks"))]
        {
            options.welcome_pointers = ENABLE_WELCOME_POINTERS
                .try_with(WelcomePointersHook::read)
                .unwrap_or(true);
            options.lifetime =
                Some(crate::identity_lifecycle::test_mocks::maybe_mock_package_lifetime());
        }
        let generated = xmtp_id::key_package::build_key_package(
            &this.inbox_id,
            this.credential,
            &this.installation_keys,
            provider,
            options,
        )
        .map_err(|error| match error {
            xmtp_id::key_package::KeyPackageConstructionError::Generation(e) => {
                IdentityError::KeyPackageGenerationError(e)
            }
            xmtp_id::key_package::KeyPackageConstructionError::InvalidExtension(e) => {
                IdentityError::InvalidExtension(e)
            }
            xmtp_id::key_package::KeyPackageConstructionError::Encode(e) => {
                IdentityError::CredentialSerialization(e)
            }
            xmtp_id::key_package::KeyPackageConstructionError::PostQuantum(e) => {
                IdentityError::GeneratePostQuantumKey(e)
            }
        })?;
        store_key_package_references(
            provider,
            generated.bundle.key_package(),
            &generated.post_quantum_keypair,
        )?;
        Ok(NewKeyPackageResult {
            key_package: generated.bundle.key_package().clone(),
            pq_pub_key: generated.post_quantum_keypair.map(|kp| kp.public),
        })
    }
}

/// The key package hash ref in the key store key layout (`kv_codec::encode_key`).
///
/// These bytes are the `KEY_PACKAGE_WRAPPER_PRIVATE_KEY` key and the
/// `key_package_history.key_package_hash_ref` column. The key layout does not
/// change with the value codec, so neither needs a migration when it does.
pub(crate) fn serialize_key_package_hash_ref(
    kp: &KeyPackage,
    provider: &impl MlsProviderExt,
) -> Result<Vec<u8>, IdentityError> {
    let key_package_hash_ref = kp
        .hash_ref(provider.crypto())
        .map_err(|_| IdentityError::UninitializedIdentity)?;
    hash_ref_key(&key_package_hash_ref)
}

/// A hash ref in the key store key layout.
fn hash_ref_key(hash_ref: &HashReference) -> Result<Vec<u8>, IdentityError> {
    kv_codec::encode_key(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, hash_ref)
        .map_err(|_| IdentityError::UninitializedIdentity)
}

// Takes a post quantum public key and returns the key used to store it in the key package references table
pub(crate) fn pq_key_package_references_key(raw_pub_key: &[u8]) -> Result<Vec<u8>, IdentityError> {
    Ok(raw_pub_key.tls_serialize_detached()?)
}

/// Decode bytes from [`serialize_key_package_hash_ref`].
///
/// The input must be exactly one length-prefixed byte string. bincode allowed
/// trailing bytes; this decoder rejects them on purpose. The only writer of
/// the column is `serialize_key_package_hash_ref`, which never writes any, and
/// the legacy archive conversion does not copy `key_package_history`. So the
/// stricter check changes no result for a stored row.
pub(crate) fn deserialize_key_package_hash_ref(
    hash_ref: &[u8],
) -> Result<HashReference, IdentityError> {
    let raw = kv_codec::key::decode_bytes(hash_ref).ok_or(IdentityError::UninitializedIdentity)?;
    // `HashReference` has no public constructor from bytes. Its TLS encoding
    // is the variable-length byte string.
    let tls = VLBytes::new(raw.to_vec()).tls_serialize_detached()?;
    HashReference::tls_deserialize_exact(tls).map_err(|_| IdentityError::UninitializedIdentity)
}

pub(crate) fn create_credential(
    inbox_id: impl AsRef<str>,
) -> Result<OpenMlsCredential, IdentityError> {
    Ok(xmtp_id::key_package::create_credential(inbox_id))
}

pub fn parse_credential(credential_bytes: &[u8]) -> Result<InboxId, IdentityError> {
    Ok(xmtp_id::key_package::parse_credential(credential_bytes)?)
}

pub fn build_post_quantum_public_key_extension(
    public_key: &[u8],
) -> Result<Extension, IdentityError> {
    Ok(xmtp_id::key_package::build_post_quantum_public_key_extension(public_key)?)
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

    // The hash ref is a value under KEY_PACKAGE_REFERENCES (value codec) and a
    // key under KEY_PACKAGE_WRAPPER_PRIVATE_KEY (key layout).
    let hash_ref = kp
        .hash_ref(provider.crypto())
        .map_err(|_| IdentityError::UninitializedIdentity)?;
    let hash_ref_value = kv_codec::encode_value(KEY_PACKAGE_REFERENCES, &hash_ref)?;
    let storage = provider.key_store();
    // Write the normal init key to the key package references
    storage.write(KEY_PACKAGE_REFERENCES, &public_init_key, &hash_ref_value)?;

    if let Some(post_quantum_keypair) = post_quantum_keypair {
        let post_quantum_public_key = pq_key_package_references_key(&post_quantum_keypair.public)?;

        // Write the post quantum wrapper encryption public key to the key package references
        storage.write(
            KEY_PACKAGE_REFERENCES,
            &post_quantum_public_key,
            &hash_ref_value,
        )?;

        // The key is borrowed, and its encoding is wiped after the write.
        storage
            .write_wrapper_private_key(&hash_ref_key(&hash_ref)?, &post_quantum_keypair.private)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::identity::{pq_key_package_references_key, serialize_key_package_hash_ref};
    use openmls_traits::OpenMlsProvider;
    use tls_codec::Serialize;
    use xmtp_db::XmtpMlsStorageProvider;
    use xmtp_db::{MlsProviderExt, sql_key_store::KEY_PACKAGE_REFERENCES};

    /// The key package hash ref has three uses. The wrapper private key's
    /// storage key and the `key_package_history` column use the key layout.
    /// The `KEY_PACKAGE_REFERENCES` value uses the value codec, CBOR. The two
    /// encodings differ, so a site that used the other one fails here. The
    /// column decodes back to the hash ref.
    #[xmtp_common::test(unwrap_try = true)]
    async fn key_package_hash_ref_uses_the_key_layout_and_cbor() {
        use super::{deserialize_key_package_hash_ref, store_key_package_references};
        use diesel::prelude::*;
        use xmtp_db::{
            ConnectionExt, XmtpTestDb, schema::openmls_key_value::dsl, sql_key_store::SqlKeyStore,
            xmtp_openmls_provider::XmtpOpenMlsProvider,
        };

        let store = xmtp_db::TestDb::create_persistent_store(None).await;
        let provider = XmtpOpenMlsProvider::new(SqlKeyStore::new(store.conn()));
        let bundle = xmtp_db::test_utils::generate_key_package_bundle();
        let post_quantum = xmtp_cryptography::post_quantum::generate_post_quantum_key()?;
        store_key_package_references(&provider, bundle.key_package(), &Some(post_quantum.clone()))?;

        let hash_ref = bundle.key_package().hash_ref(provider.crypto())?;
        let raw = hash_ref.as_slice();
        // The key layout: a `u64` LE length, then the bytes.
        let key_layout = [&(raw.len() as u64).to_le_bytes()[..], raw].concat();
        // CBOR: {"value": {"vec": [the bytes as integers]}}.
        let mut cbor = [
            &[0xa1, 0x65][..],
            b"value",
            &[0xa1, 0x63],
            b"vec",
            &[0x98, u8::try_from(raw.len())?],
        ]
        .concat();
        for &byte in raw {
            if byte >= 0x18 {
                cbor.push(0x18);
            }
            cbor.push(byte);
        }
        assert_ne!(key_layout, cbor);

        let column = serialize_key_package_hash_ref(bundle.key_package(), &provider)?;
        assert_eq!(column, key_layout, "key_package_history column");
        assert_eq!(deserialize_key_package_hash_ref(&column)?, hash_ref);
        assert!(deserialize_key_package_hash_ref(&column[..column.len() - 1]).is_err());
        // Strict on purpose: trailing bytes are rejected.
        assert!(deserialize_key_package_hash_ref(&[column.as_slice(), &[0]].concat()).is_err());
        assert!(deserialize_key_package_hash_ref(&cbor).is_err());

        let stored = |key: &[u8]| -> Option<Vec<u8>> {
            let storage_key = [KEY_PACKAGE_REFERENCES, key, &1u16.to_be_bytes()].concat();
            store
                .conn()
                .raw_query(|conn| {
                    dsl::openmls_key_value
                        .filter(dsl::key_bytes.eq(storage_key))
                        .select(dsl::value_bytes)
                        .first::<Vec<u8>>(conn)
                        .optional()
                })
                .unwrap()
        };
        let init_key = bundle
            .key_package()
            .hpke_init_key()
            .tls_serialize_detached()?;
        let post_quantum_key = pq_key_package_references_key(&post_quantum.public)?;
        assert_eq!(stored(&init_key), Some(cbor.clone()), "init key reference");
        assert_eq!(
            stored(&post_quantum_key),
            Some(cbor.clone()),
            "post-quantum key reference"
        );
        let wrapper_key = provider.key_store().wrapper_private_key(&key_layout)?;
        assert_eq!(
            wrapper_key.as_deref().map(Vec::as_slice),
            Some(&*post_quantum.private)
        );
        assert!(provider.key_store().wrapper_private_key(&cbor)?.is_none());
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
        let err = IdentityError::UninitializedIdentity;
        assert_eq!(err.error_code(), "IdentityError::UninitializedIdentity");

        let err = IdentityError::RequiredIdentityNotFound;
        assert_eq!(err.error_code(), "IdentityError::RequiredIdentityNotFound");

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
}
