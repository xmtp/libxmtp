use std::{
    collections::HashMap,
    mem::Discriminant,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use futures::stream::{self, FuturesUnordered, StreamExt};
use openmls::{
    credentials::errors::BasicCredentialError,
    framing::{MlsMessageBodyIn, MlsMessageIn},
    group::GroupEpoch,
    messages::Welcome,
    prelude::tls_codec::{Deserialize, Error as TlsCodecError},
};
use openmls_traits::OpenMlsProvider;
use prost::EncodeError;
use thiserror::Error;

use xmtp_cryptography::signature::{sanitize_evm_addresses, AddressValidationError};
use xmtp_id::{
    associations::{
        builder::{SignatureRequest, SignatureRequestError},
        AssociationError, AssociationState, SignatureError,
    },
    scw_verifier::{RemoteSignatureVerifier, SmartContractSignatureVerifier},
    InboxId,
};

use xmtp_proto::xmtp::mls::api::v1::{
    welcome_message::{Version as WelcomeMessageVersion, V1 as WelcomeMessageV1},
    GroupMessage, WelcomeMessage,
};

use crate::{
    api::ApiClientWrapper,
    groups::{
        group_permissions::PolicySet, scoped_client::LocalScopedGroupClient,
        validated_commit::CommitValidationError, GroupError, GroupMetadataOptions, IntentError,
        MlsGroup,
    },
    identity::{parse_credential, Identity, IdentityError},
    identity_updates::{load_identity_updates, IdentityUpdateError},
    intents::Intents,
    mutex_registry::MutexRegistry,
    retry::Retry,
    retry_async, retryable,
    storage::{
        consent_record::{ConsentState, ConsentType, StoredConsentRecord},
        db_connection::DbConnection,
        group::{GroupMembershipState, GroupQueryArgs, StoredGroup},
        group_message::StoredGroupMessage,
        refresh_state::EntityKind,
        sql_key_store, EncryptedMessageStore, StorageError,
    },
    subscriptions::{EventError, LocalEvents, SafeBroadcast},
    verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Fetch, XmtpApi,
};

/// Enum representing the network the Client is connected to
#[derive(Clone, Copy, Default, Debug)]
pub enum Network {
    Local(&'static str),
    #[default]
    Dev,
    Prod,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    AddressValidation(#[from] AddressValidationError),
    #[error("could not publish: {0}")]
    PublishError(String),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("dieselError: {0}")]
    Diesel(#[from] diesel::result::Error),
    #[error("Query failed: {0}")]
    QueryError(#[from] xmtp_proto::api_client::Error),
    #[error("API error: {0}")]
    Api(#[from] crate::api::WrappedApiError),
    #[error("identity error: {0}")]
    Identity(#[from] crate::identity::IdentityError),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error("key package verification: {0}")]
    KeyPackageVerification(#[from] KeyPackageVerificationError),
    #[error("syncing errors: {0:?}")]
    SyncingError(Vec<MessageProcessingError>),
    #[error("Stream inconsistency error: {0}")]
    StreamInconsistency(String),
    #[error("Association error: {0}")]
    Association(#[from] AssociationError),
    #[error("signature validation error: {0}")]
    SignatureValidation(#[from] SignatureError),
    #[error(transparent)]
    IdentityUpdate(#[from] IdentityUpdateError),
    #[error(transparent)]
    SignatureRequest(#[from] SignatureRequestError),
    // the box is to prevent infinite cycle between client and group errors
    #[error(transparent)]
    Group(Box<GroupError>),
    #[error(transparent)]
    LocalEvent(#[from] EventError),
    #[error("generic:{0}")]
    Generic(String),
}

impl From<GroupError> for ClientError {
    fn from(err: GroupError) -> ClientError {
        ClientError::Group(Box::new(err))
    }
}

impl crate::retry::RetryableError for ClientError {
    fn is_retryable(&self) -> bool {
        match self {
            ClientError::Group(group_error) => retryable!(group_error),
            ClientError::Diesel(diesel_error) => retryable!(diesel_error),
            ClientError::Api(api_error) => retryable!(api_error),
            ClientError::Storage(storage_error) => retryable!(storage_error),
            ClientError::Generic(err) => err.contains("database is locked"),
            _ => false,
        }
    }
}

/// Errors that can occur when reading and processing a message off the network
#[derive(Debug, Error)]
pub enum MessageProcessingError {
    #[error("[{0}] already processed")]
    AlreadyProcessed(u64),
    #[error("diesel error: {0}")]
    Diesel(#[from] diesel::result::Error),
    #[error("[{message_time_ns:?}] invalid sender with credential: {credential:?}")]
    InvalidSender {
        message_time_ns: u64,
        credential: Vec<u8>,
    },
    #[error("invalid payload")]
    InvalidPayload,
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error("openmls process message error: {0}")]
    OpenMlsProcessMessage(#[from] openmls::prelude::ProcessMessageError),
    #[error("merge staged commit: {0}")]
    MergeStagedCommit(#[from] openmls::group::MergeCommitError<sql_key_store::SqlKeyStoreError>),
    #[error(
        "no pending commit to merge. group epoch is {group_epoch:?} and got {message_epoch:?}"
    )]
    NoPendingCommit {
        message_epoch: GroupEpoch,
        group_epoch: GroupEpoch,
    },
    #[error("intent error: {0}")]
    Intent(#[from] IntentError),
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error("unsupported message type: {0:?}")]
    UnsupportedMessageType(Discriminant<MlsMessageBodyIn>),
    #[error("commit validation")]
    CommitValidation(#[from] CommitValidationError),
    #[error("codec")]
    Codec(#[from] crate::codecs::CodecError),
    #[error("encode proto: {0}")]
    EncodeProto(#[from] EncodeError),
    #[error("epoch increment not allowed")]
    EpochIncrementNotAllowed,
    #[error("Welcome processing error: {0}")]
    WelcomeProcessing(Box<GroupError>),
    #[error("wrong credential type")]
    WrongCredentialType(#[from] BasicCredentialError),
    #[error("proto decode error: {0}")]
    DecodeError(#[from] prost::DecodeError),
    #[error("clear pending commit error: {0}")]
    ClearPendingCommit(#[from] sql_key_store::SqlKeyStoreError),
    #[error(transparent)]
    Group(#[from] Box<GroupError>),
    #[error("Serialization/Deserialization Error {0}")]
    Serde(#[from] serde_json::Error),
    #[error("generic:{0}")]
    Generic(String),
    #[error("intent is missing staged_commit field")]
    IntentMissingStagedCommit,
    #[error(transparent)]
    Deserialization(#[from] xmtp_id::associations::DeserializationError),
}

impl crate::retry::RetryableError for MessageProcessingError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Group(group_error) => retryable!(group_error),
            Self::Identity(identity_error) => retryable!(identity_error),
            Self::OpenMlsProcessMessage(err) => retryable!(err),
            Self::MergeStagedCommit(err) => retryable!(err),
            Self::Diesel(diesel_error) => retryable!(diesel_error),
            Self::Storage(s) => retryable!(s),
            Self::Generic(err) => err.contains("database is locked"),
            _ => false,
        }
    }
}

impl From<String> for ClientError {
    fn from(value: String) -> Self {
        Self::Generic(value)
    }
}

impl From<&str> for ClientError {
    fn from(value: &str) -> Self {
        Self::Generic(value.to_string())
    }
}

/// Clients manage access to the network, identity, and data store
pub struct Client<ApiClient, V = RemoteSignatureVerifier<ApiClient>> {
    pub(crate) api_client: Arc<ApiClientWrapper<ApiClient>>,
    pub(crate) intents: Arc<Intents>,
    pub(crate) context: Arc<XmtpMlsLocalContext>,
    pub(crate) history_sync_url: Option<String>,
    pub(crate) local_events: Arc<SafeBroadcast<Self>>,
    /// The method of verifying smart contract wallet signatures for this Client
    pub(crate) scw_verifier: Arc<V>,
}

// most of these things are `Arc`'s
impl<ApiClient, V> Clone for Client<ApiClient, V> {
    fn clone(&self) -> Self {
        Self {
            api_client: self.api_client.clone(),
            context: self.context.clone(),
            history_sync_url: self.history_sync_url.clone(),
            local_events: self.local_events.clone(),
            scw_verifier: self.scw_verifier.clone(),
            intents: self.intents.clone(),
        }
    }
}

/// The local context a XMTP MLS needs to function:
/// - Sqlite Database
/// - Identity for the User
pub struct XmtpMlsLocalContext {
    /// XMTP Identity
    pub(crate) identity: Identity,
    /// XMTP Local Storage
    store: EncryptedMessageStore,
    pub(crate) mutexes: MutexRegistry,
}

impl XmtpMlsLocalContext {
    /// The installation public key is the primary identifier for an installation
    pub fn installation_public_key(&self) -> Vec<u8> {
        self.identity.installation_keys.public_slice().to_vec()
    }

    /// Get the account address of the blockchain account associated with this client
    pub fn inbox_id(&self) -> InboxId {
        self.identity.inbox_id().clone()
    }

    /// Get sequence id, may not be consistent with the backend
    pub fn inbox_sequence_id(&self, conn: &DbConnection) -> Result<i64, StorageError> {
        self.identity.sequence_id(conn)
    }

    pub fn store(&self) -> &EncryptedMessageStore {
        &self.store
    }

    /// Pulls a new database connection and creates a new provider
    pub fn mls_provider(&self) -> Result<XmtpOpenMlsProvider, ClientError> {
        Ok(self.store.conn()?.into())
    }

    /// Integrators should always check the `signature_request` return value of this function before calling [`register_identity`](Self::register_identity).
    /// If `signature_request` returns `None`, then the wallet signature is not required and [`register_identity`](Self::register_identity) can be called with None as an argument.
    pub fn signature_request(&self) -> Option<SignatureRequest> {
        self.identity.signature_request()
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    /// Create a new client with the given network, identity, and store.
    /// It is expected that most users will use the [`ClientBuilder`](crate::builder::ClientBuilder) instead of instantiating
    /// a client directly.
    pub fn new(
        api_client: ApiClientWrapper<ApiClient>,
        identity: Identity,
        store: EncryptedMessageStore,
        scw_verifier: V,
        history_sync_url: Option<String>,
        local_events: Arc<SafeBroadcast<Self>>,
    ) -> Self
    where
        V: SmartContractSignatureVerifier,
    {
        let context = Arc::new(XmtpMlsLocalContext {
            identity,
            store,
            mutexes: MutexRegistry::new(),
        });
        let intents = Arc::new(Intents {
            context: context.clone(),
        });

        Self {
            api_client: api_client.into(),
            context,
            history_sync_url,
            local_events,
            scw_verifier: scw_verifier.into(),
            intents,
        }
    }

    pub fn scw_verifier(&self) -> &V {
        &self.scw_verifier
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    /// Retrieves the client's installation public key, sometimes also called `installation_id`
    pub fn installation_public_key(&self) -> Vec<u8> {
        self.context.installation_public_key()
    }
    /// Retrieves the client's inbox ID
    pub fn inbox_id(&self) -> String {
        self.context.inbox_id()
    }

    pub fn intents(&self) -> &Arc<Intents> {
        &self.intents
    }

    /// Pulls a connection and creates a new MLS Provider
    pub fn mls_provider(&self) -> Result<XmtpOpenMlsProvider, ClientError> {
        self.context.mls_provider()
    }

    pub fn history_sync_url(&self) -> Option<&String> {
        self.history_sync_url.as_ref()
    }

    /// Calls the server to look up the `inbox_id` associated with a given address
    pub async fn find_inbox_id_from_address(
        &self,
        address: String,
    ) -> Result<Option<String>, ClientError> {
        let results = self.find_inbox_ids_from_addresses(&[address]).await?;
        if let Some(first_result) = results.into_iter().next() {
            Ok(first_result)
        } else {
            Ok(None)
        }
    }

    /// Calls the server to look up the `inbox_id`s` associated with a list of addresses.
    /// If no `inbox_id` is found, returns None.
    pub async fn find_inbox_ids_from_addresses(
        &self,
        addresses: &[String],
    ) -> Result<Vec<Option<String>>, ClientError> {
        let sanitized_addresses = sanitize_evm_addresses(addresses)?;
        let mut results = self
            .api_client
            .get_inbox_ids(sanitized_addresses.clone())
            .await?;
        let inbox_ids: Vec<Option<String>> = sanitized_addresses
            .into_iter()
            .map(|address| results.remove(&address))
            .collect();

        Ok(inbox_ids)
    }

    /// Get the highest `sequence_id` from the local database for the client's `inbox_id`.
    /// This may not be consistent with the latest state on the backend.
    pub fn inbox_sequence_id(&self, conn: &DbConnection) -> Result<i64, StorageError> {
        self.context.inbox_sequence_id(conn)
    }

    /// Get the [`AssociationState`] for the client's `inbox_id`
    pub async fn inbox_state(
        &self,
        refresh_from_network: bool,
    ) -> Result<AssociationState, ClientError> {
        let conn = self.store().conn()?;
        let inbox_id = self.inbox_id();
        if refresh_from_network {
            load_identity_updates(&self.api_client, &conn, vec![inbox_id.clone()]).await?;
        }
        let state = self.get_association_state(&conn, inbox_id, None).await?;
        Ok(state)
    }

    /// Get the [`AssociationState`] for each `inbox_id`
    pub async fn inbox_addresses<InboxId: AsRef<str>>(
        &self,
        refresh_from_network: bool,
        inbox_ids: Vec<InboxId>,
    ) -> Result<Vec<AssociationState>, ClientError> {
        let conn = self.store().conn()?;
        if refresh_from_network {
            load_identity_updates(
                &self.api_client,
                &conn,
                inbox_ids.iter().map(|s| String::from(s.as_ref())).collect(),
            )
            .await?;
        }
        let state = self
            .batch_get_association_state(
                &conn,
                &inbox_ids.into_iter().map(|i| (i, None)).collect::<Vec<_>>(),
            )
            .await?;
        Ok(state)
    }

    /// Set a consent record in the local database.
    /// If the consent record is an address set the consent state for both the address and `inbox_id`
    pub async fn set_consent_states(
        &self,
        records: &[StoredConsentRecord],
    ) -> Result<(), ClientError> {
        let conn = self.store().conn()?;

        let mut new_records = Vec::new();
        let mut addresses_to_lookup = Vec::new();
        let mut record_indices = Vec::new();

        for (index, record) in records.iter().enumerate() {
            if record.entity_type == ConsentType::Address {
                addresses_to_lookup.push(record.entity.clone());
                record_indices.push(index);
            }
        }

        let inbox_ids = self
            .find_inbox_ids_from_addresses(&addresses_to_lookup)
            .await?;

        for (i, inbox_id_opt) in inbox_ids.into_iter().enumerate() {
            if let Some(inbox_id) = inbox_id_opt {
                let record = &records[record_indices[i]];
                new_records.push(StoredConsentRecord::new(
                    ConsentType::InboxId,
                    record.state,
                    inbox_id,
                ));
            }
        }

        conn.insert_or_replace_consent_records(records)?;
        conn.insert_or_replace_consent_records(&new_records)?;

        let local_events = self.local_events();
        for record in records {
            local_events.send(LocalEvents::ConsentUpdate(record.clone()))?;
        }
        for record in new_records {
            local_events.send(LocalEvents::ConsentUpdate(record))?;
        }

        Ok(())
    }

    /// Get the consent state for a given entity
    pub async fn get_consent_state(
        &self,
        entity_type: ConsentType,
        entity: String,
    ) -> Result<ConsentState, ClientError> {
        let conn = self.store().conn()?;
        let record = if entity_type == ConsentType::Address {
            if let Some(inbox_id) = self.find_inbox_id_from_address(entity.clone()).await? {
                conn.get_consent_record(inbox_id, ConsentType::InboxId)?
            } else {
                conn.get_consent_record(entity, entity_type)?
            }
        } else {
            conn.get_consent_record(entity, entity_type)?
        };

        match record {
            Some(rec) => Ok(rec.state),
            None => Ok(ConsentState::Unknown),
        }
    }

    /// Gets a reference to the client's store
    pub fn store(&self) -> &EncryptedMessageStore {
        &self.context.store
    }

    /// Release the client's database connection
    pub fn release_db_connection(&self) -> Result<(), ClientError> {
        let store = &self.context.store;
        store.release_connection()?;
        Ok(())
    }

    /// Reconnect to the client's database if it has previously been released
    pub fn reconnect_db(&self) -> Result<(), ClientError> {
        self.context.store.reconnect()?;
        Ok(())
    }
    /// Get a reference to the client's identity struct
    pub fn identity(&self) -> &Identity {
        &self.context.identity
    }

    /// Get a reference (in an Arc) to the client's local context
    pub fn context(&self) -> &Arc<XmtpMlsLocalContext> {
        &self.context
    }

    /// Create a new group with the default settings
    /// Applies a custom [`PolicySet`] to the group if one is specified
    pub fn create_group(
        &self,
        permissions_policy_set: Option<PolicySet>,
        opts: GroupMetadataOptions,
    ) -> Result<MlsGroup<Self>, ClientError> {
        tracing::info!("creating group");

        let group: MlsGroup<Client<ApiClient, V>> = MlsGroup::create_and_insert(
            Arc::new(self.clone()),
            GroupMembershipState::Allowed,
            permissions_policy_set.unwrap_or_default(),
            opts,
        )?;

        // notify streams of our new group
        let _ = self.local_events.send(LocalEvents::NewGroup(group.clone()));

        Ok(group)
    }

    /// Create a group with an initial set of members added
    pub async fn create_group_with_members(
        &self,
        account_addresses: &[String],
        permissions_policy_set: Option<PolicySet>,
        opts: GroupMetadataOptions,
    ) -> Result<MlsGroup<Self>, ClientError> {
        tracing::info!("creating group");
        let group = self.create_group(permissions_policy_set, opts)?;

        group.add_members(account_addresses).await?;

        Ok(group)
    }

    /// Create a new Direct Message with the default settings
    pub async fn create_dm(&self, account_address: String) -> Result<MlsGroup<Self>, ClientError> {
        tracing::info!("creating dm with address: {}", account_address);

        let inbox_id = match self
            .find_inbox_id_from_address(account_address.clone())
            .await?
        {
            Some(id) => id,
            None => {
                return Err(ClientError::Storage(StorageError::NotFound(format!(
                    "inbox id for address {} not found",
                    account_address
                ))))
            }
        };

        self.create_dm_by_inbox_id(inbox_id).await
    }

    /// Create a new Direct Message with the default settings
    pub async fn create_dm_by_inbox_id(
        &self,
        dm_target_inbox_id: InboxId,
    ) -> Result<MlsGroup<Self>, ClientError> {
        tracing::info!("creating dm with {}", dm_target_inbox_id);

        let group: MlsGroup<Client<ApiClient, V>> = MlsGroup::create_dm_and_insert(
            Arc::new(self.clone()),
            GroupMembershipState::Allowed,
            dm_target_inbox_id.clone(),
        )?;

        group.add_members_by_inbox_id(&[dm_target_inbox_id]).await?;

        // notify any streams of the new group
        let _ = self.local_events.send(LocalEvents::NewGroup(group.clone()));

        Ok(group)
    }

    pub(crate) fn create_sync_group(&self) -> Result<MlsGroup<Self>, ClientError> {
        tracing::info!("creating sync group");
        let sync_group = MlsGroup::create_and_insert_sync_group(Arc::new(self.clone()))?;

        Ok(sync_group)
    }

    /**
     * Look up a group by its ID
     *
     * Returns a [`MlsGroup`] if the group exists, or an error if it does not
     */
    pub fn group(&self, group_id: Vec<u8>) -> Result<MlsGroup<Self>, ClientError> {
        let conn = &mut self.store().conn()?;
        let stored_group: Option<StoredGroup> = conn.fetch(&group_id)?;
        match stored_group {
            Some(group) => Ok(MlsGroup::new(self.clone(), group.id, group.created_at_ns)),
            None => Err(ClientError::Storage(StorageError::NotFound(format!(
                "group {}",
                hex::encode(group_id)
            )))),
        }
    }

    /**
     * Look up a DM group by the target's inbox_id.
     *
     * Returns a [`MlsGroup`] if the group exists, or an error if it does not
     */
    pub fn dm_group_from_target_inbox(
        &self,
        target_inbox_id: String,
    ) -> Result<MlsGroup<Self>, ClientError> {
        let conn = self.store().conn()?;
        match conn.find_dm_group(&target_inbox_id)? {
            Some(dm_group) => Ok(MlsGroup::new(
                self.clone(),
                dm_group.id,
                dm_group.created_at_ns,
            )),
            None => Err(ClientError::Storage(StorageError::NotFound(format!(
                "dm_target_inbox_id {}",
                hex::encode(target_inbox_id)
            )))),
        }
    }

    /// Look up a message by its ID
    /// Returns a [`StoredGroupMessage`] if the message exists, or an error if it does not
    pub fn message(&self, message_id: Vec<u8>) -> Result<StoredGroupMessage, ClientError> {
        let conn = &mut self.store().conn()?;
        let message = conn.get_group_message(&message_id)?;
        match message {
            Some(message) => Ok(message),
            None => Err(ClientError::Storage(StorageError::NotFound(format!(
                "message {}",
                hex::encode(message_id)
            )))),
        }
    }

    /// Query for groups with optional filters
    ///
    /// Filters:
    /// - allowed_states: only return groups with the given membership states
    /// - created_after_ns: only return groups created after the given timestamp (in nanoseconds)
    /// - created_before_ns: only return groups created before the given timestamp (in nanoseconds)
    /// - limit: only return the first `limit` groups
    pub fn find_groups(&self, args: GroupQueryArgs) -> Result<Vec<MlsGroup<Self>>, ClientError> {
        Ok(self
            .store()
            .conn()?
            .find_groups(args)?
            .into_iter()
            .map(|stored_group| {
                MlsGroup::new(self.clone(), stored_group.id, stored_group.created_at_ns)
            })
            .collect())
    }

    /// Upload a Key Package to the network and publish the signed identity update
    /// from the provided SignatureRequest
    pub async fn register_identity(
        &self,
        signature_request: SignatureRequest,
    ) -> Result<(), ClientError> {
        tracing::info!("registering identity");
        // Register the identity before applying the signature request
        let provider: XmtpOpenMlsProvider = self.store().conn()?.into();

        self.identity()
            .register(&provider, &self.api_client)
            .await?;

        self.apply_signature_request(signature_request).await?;

        Ok(())
    }

    /// Upload a new key package to the network replacing an existing key package
    /// This is expected to be run any time the client receives new Welcome messages
    pub async fn rotate_key_package(&self) -> Result<(), ClientError> {
        self.store()
            .transaction_async(|provider| async move {
                self.identity()
                    .rotate_key_package(&provider, &self.api_client)
                    .await
            })
            .await?;

        Ok(())
    }

    /// Query for group messages that have a `sequence_id` > than the highest cursor
    /// found in the local database
    pub(crate) async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection,
    ) -> Result<Vec<GroupMessage>, ClientError> {
        let id_cursor = conn.get_last_cursor_for_id(group_id, EntityKind::Group)?;

        let welcomes = self
            .api_client
            .query_group_messages(group_id.to_vec(), Some(id_cursor as u64))
            .await?;

        Ok(welcomes)
    }

    /// Query for welcome messages that have a `sequence_id` > than the highest cursor
    /// found in the local database
    pub(crate) async fn query_welcome_messages(
        &self,
        conn: &DbConnection,
    ) -> Result<Vec<WelcomeMessage>, ClientError> {
        let installation_id = self.installation_public_key();
        let id_cursor = conn.get_last_cursor_for_id(&installation_id, EntityKind::Welcome)?;

        let welcomes = self
            .api_client
            .query_welcome_messages(installation_id, Some(id_cursor as u64))
            .await?;

        Ok(welcomes)
    }

    /// Fetches the current key package from the network for each of the `installation_id`s specified
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<VerifiedKeyPackageV2>, ClientError> {
        let key_package_results = self.api_client.fetch_key_packages(installation_ids).await?;

        let mls_provider = self.mls_provider()?;
        Ok(key_package_results
            .values()
            .map(|bytes| VerifiedKeyPackageV2::from_bytes(mls_provider.crypto(), bytes.as_slice()))
            .collect::<Result<_, _>>()?)
    }

    /// Download all unread welcome messages and converts to a group struct, ignoring malformed messages.
    /// Returns any new groups created in the operation
    pub async fn sync_welcomes(
        &self,
        conn: &DbConnection,
    ) -> Result<Vec<MlsGroup<Self>>, ClientError> {
        let envelopes = self.query_welcome_messages(conn).await?;
        let num_envelopes = envelopes.len();
        let id = self.installation_public_key();

        let groups: Vec<MlsGroup<Self>> = stream::iter(envelopes.into_iter())
            .filter_map(|envelope: WelcomeMessage| async {
                let welcome_v1 = match extract_welcome_message(envelope) {
                    Ok(inner) => inner,
                    Err(err) => {
                        tracing::error!("failed to extract welcome message: {}", err);
                        return None;
                    }
                };
                retry_async!(
                    Retry::default(),
                    (async {
                        let welcome_v1 = &welcome_v1;
                        self.intents.process_for_id(
                            &id,
                            EntityKind::Welcome,
                            welcome_v1.id,
                            |provider| async move {
                                let result = MlsGroup::create_from_encrypted_welcome(
                                    Arc::new(self.clone()),
                                    &provider,
                                    welcome_v1.hpke_public_key.as_slice(),
                                    &welcome_v1.data,
                                    welcome_v1.id as i64,
                                )
                                .await;

                                match result {
                                    Ok(mls_group) => Ok(Some(mls_group)),
                                    Err(err) => {
                                        use crate::StorageError::*;
                                        use crate::DuplicateItem::*;

                                        if matches!(err, GroupError::Storage(Duplicate(WelcomeId(_)))) {
                                            tracing::warn!("failed to create group from welcome due to duplicate welcome ID: {}", err);
                                        } else {
                                            tracing::error!("failed to create group from welcome: {}", err);
                                        }

                                        Err(MessageProcessingError::WelcomeProcessing(
                                            Box::new(err)
                                        ))
                                    }
                                }
                            },
                        )
                        .await
                    })
                )
                .ok()
                .flatten()
            })
            .collect()
            .await;

        // If any welcomes were found, rotate your key package
        if num_envelopes > 0 {
            self.rotate_key_package().await?;
        }

        Ok(groups)
    }

    /// Sync all groups for the current installation and return the number of groups that were synced.
    /// Only active groups will be synced.
    pub async fn sync_all_groups(&self, groups: Vec<MlsGroup<Self>>) -> Result<usize, GroupError> {
        // Acquire a single connection to be reused
        let provider: XmtpOpenMlsProvider = self.mls_provider()?;

        let active_group_count = Arc::new(AtomicUsize::new(0));

        let sync_futures = groups
            .into_iter()
            .map(|group| {
                // create new provider ref that gets moved, leaving original
                // provider alone.
                let provider_ref = &provider;
                let active_group_count = Arc::clone(&active_group_count);
                async move {
                    let mls_group = group.load_mls_group(provider_ref)?;
                    tracing::info!("[{}] syncing group", self.inbox_id());
                    tracing::info!(
                        "current epoch for [{}] in sync_all_groups() is Epoch: [{}]",
                        self.inbox_id(),
                        mls_group.epoch()
                    );
                    if mls_group.is_active() {
                        group.maybe_update_installations(provider_ref, None).await?;

                        group.sync_with_conn(provider_ref).await?;
                        active_group_count.fetch_add(1, Ordering::SeqCst);
                    }

                    Ok::<(), GroupError>(())
                }
            })
            .collect::<FuturesUnordered<_>>();

        sync_futures
            .collect::<Vec<Result<_, _>>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        Ok(active_group_count.load(Ordering::SeqCst))
    }

    /**
     * Validates a credential against the given installation public key
     *
     * This will go to the network and get the latest association state for the inbox.
     * It ensures that the installation_pub_key is in that association state
     */
    pub async fn validate_credential_against_network(
        &self,
        conn: &DbConnection,
        credential: &[u8],
        installation_pub_key: Vec<u8>,
    ) -> Result<InboxId, ClientError> {
        let inbox_id = parse_credential(credential)?;
        let association_state = self.get_latest_association_state(conn, &inbox_id).await?;

        match association_state.get(&installation_pub_key.clone().into()) {
            Some(_) => Ok(inbox_id),
            None => Err(IdentityError::InstallationIdNotFound(inbox_id).into()),
        }
    }

    /// Check whether an account_address has a key package registered on the network
    ///
    /// Arguments:
    /// - account_addresses: a list of account addresses to check
    ///
    /// Returns:
    /// A Vec of booleans indicating whether each account address has a key package registered on the network
    pub async fn can_message(
        &self,
        account_addresses: &[String],
    ) -> Result<HashMap<String, bool>, ClientError> {
        let account_addresses = sanitize_evm_addresses(account_addresses)?;
        let inbox_id_map = self
            .api_client
            .get_inbox_ids(account_addresses.clone())
            .await?;

        let results = account_addresses
            .iter()
            .map(|address| {
                let result = inbox_id_map.get(address).map(|_| true).unwrap_or(false);
                (address.clone(), result)
            })
            .collect::<HashMap<String, bool>>();

        Ok(results)
    }
}

pub(crate) fn extract_welcome_message(
    welcome: WelcomeMessage,
) -> Result<WelcomeMessageV1, ClientError> {
    match welcome.version {
        Some(WelcomeMessageVersion::V1(welcome)) => Ok(welcome),
        _ => Err(ClientError::Generic(
            "unexpected message type in welcome".to_string(),
        )),
    }
}

pub fn deserialize_welcome(welcome_bytes: &Vec<u8>) -> Result<Welcome, ClientError> {
    // let welcome_proto = WelcomeMessageProto::decode(&mut welcome_bytes.as_slice())?;
    let welcome = MlsMessageIn::tls_deserialize(&mut welcome_bytes.as_slice())?;
    match welcome.extract() {
        MlsMessageBodyIn::Welcome(welcome) => Ok(welcome),
        _ => Err(ClientError::Generic(
            "unexpected message type in welcome".to_string(),
        )),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::Client;
    use diesel::RunQueryDsl;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::{scw_verifier::SmartContractSignatureVerifier, InboxOwner};

    use crate::{
        builder::ClientBuilder,
        groups::GroupMetadataOptions,
        hpke::{decrypt_welcome, encrypt_welcome},
        identity::serialize_key_package_hash_ref,
        storage::{
            consent_record::{ConsentState, ConsentType, StoredConsentRecord},
            group::GroupQueryArgs,
            group_message::MsgQueryArgs,
            schema::identity_updates,
        },
        XmtpApi,
    };

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_group_member_recovery() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_wallet = generate_local_wallet();
        // Add two separate installations for Bola
        let bola_a = ClientBuilder::new_test_client(&bola_wallet).await;
        let bola_b = ClientBuilder::new_test_client(&bola_wallet).await;
        let group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        // Add both of Bola's installations to the group
        group
            .add_members_by_inbox_id(&[bola_a.inbox_id(), bola_b.inbox_id()])
            .await
            .unwrap();

        let conn = amal.store().conn().unwrap();
        conn.raw_query(|conn| diesel::delete(identity_updates::table).execute(conn))
            .unwrap();

        let members = group.members().await.unwrap();
        // // The three installations should count as two members
        assert_eq!(members.len(), 2);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_mls_error() {
        tracing::debug!("Test MLS Error");
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let result = client
            .api_client
            .upload_key_package(vec![1, 2, 3], false)
            .await;

        assert!(result.is_err());
        let error_string = result.err().unwrap().to_string();
        assert!(error_string.contains("invalid identity"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_register_installation() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        // Make sure the installation is actually on the network
        let association_state = client_2
            .get_latest_association_state(&client_2.store().conn().unwrap(), client.inbox_id())
            .await
            .unwrap();

        assert_eq!(association_state.installation_ids().len(), 1);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 1)
    )]
    async fn test_rotate_key_package() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;

        // Get original KeyPackage.
        let kp1 = client
            .get_key_packages_for_installation_ids(vec![client.installation_public_key()])
            .await
            .unwrap();
        assert_eq!(kp1.len(), 1);
        let init1 = kp1[0].inner.hpke_init_key();

        // Rotate and fetch again.
        client.rotate_key_package().await.unwrap();

        let kp2 = client
            .get_key_packages_for_installation_ids(vec![client.installation_public_key()])
            .await
            .unwrap();
        assert_eq!(kp2.len(), 1);
        let init2 = kp2[0].inner.hpke_init_key();

        assert_ne!(init1, init2);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_find_groups() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group_1 = client
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        let group_2 = client
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let groups = client.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].group_id, group_1.group_id);
        assert_eq!(groups[1].group_id, group_2.group_id);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_find_inbox_id() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert_eq!(
            client
                .find_inbox_id_from_address(wallet.get_address())
                .await
                .unwrap(),
            Some(client.inbox_id())
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 2)
    )]
    async fn test_sync_welcomes() {
        let alice = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bob = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alice_bob_group = alice
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alice_bob_group
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();

        let bob_received_groups = bob
            .sync_welcomes(&bob.store().conn().unwrap())
            .await
            .unwrap();
        assert_eq!(bob_received_groups.len(), 1);
        assert_eq!(
            bob_received_groups.first().unwrap().group_id,
            alice_bob_group.group_id
        );

        let duplicate_received_groups = bob
            .sync_welcomes(&bob.store().conn().unwrap())
            .await
            .unwrap();
        assert_eq!(duplicate_received_groups.len(), 0);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 2)
    )]
    async fn test_sync_all_groups() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alix_bo_group1 = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        let alix_bo_group2 = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_bo_group1
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();
        alix_bo_group2
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        let bob_received_groups = bo.sync_welcomes(&bo.store().conn().unwrap()).await.unwrap();
        assert_eq!(bob_received_groups.len(), 2);

        let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
        let bo_group1 = bo.group(alix_bo_group1.clone().group_id).unwrap();
        let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages1.len(), 0);
        let bo_group2 = bo.group(alix_bo_group2.clone().group_id).unwrap();
        let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages2.len(), 0);
        alix_bo_group1
            .send_message(vec![1, 2, 3].as_slice())
            .await
            .unwrap();
        alix_bo_group2
            .send_message(vec![1, 2, 3].as_slice())
            .await
            .unwrap();

        bo.sync_all_groups(bo_groups).await.unwrap();

        let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages1.len(), 1);
        let bo_group2 = bo.group(alix_bo_group2.clone().group_id).unwrap();
        let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages2.len(), 1);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 1)
    )]
    async fn test_welcome_encryption() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let provider = client.mls_provider().unwrap();

        let kp = client.identity().new_key_package(&provider).unwrap();
        let hpke_public_key = kp.hpke_init_key().as_slice();
        let to_encrypt = vec![1, 2, 3];

        // Encryption doesn't require any details about the sender, so we can test using one client
        let encrypted = encrypt_welcome(to_encrypt.as_slice(), hpke_public_key).unwrap();

        let decrypted = decrypt_welcome(&provider, hpke_public_key, encrypted.as_slice()).unwrap();

        assert_eq!(decrypted, to_encrypt);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 1)
    )]
    async fn test_add_remove_then_add_again() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Create a group and invite bola
        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();
        assert_eq!(amal_group.members().await.unwrap().len(), 2);

        // Now remove bola
        amal_group
            .remove_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();
        assert_eq!(amal_group.members().await.unwrap().len(), 1);
        tracing::info!("Syncing bolas welcomes");
        // See if Bola can see that they were added to the group
        bola.sync_welcomes(&bola.store().conn().unwrap())
            .await
            .unwrap();
        let bola_groups = bola.find_groups(Default::default()).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group = bola_groups.first().unwrap();
        tracing::info!("Syncing bolas messages");
        bola_group.sync().await.unwrap();
        // TODO: figure out why Bola's status is not updating to be inactive
        // assert!(!bola_group.is_active().unwrap());

        // Bola should have one readable message (them being added to the group)
        let mut bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();

        assert_eq!(bola_messages.len(), 1);

        // Add Bola back to the group
        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();
        bola.sync_welcomes(&bola.store().conn().unwrap())
            .await
            .unwrap();

        // Send a message from Amal, now that Bola is back in the group
        amal_group
            .send_message(vec![1, 2, 3].as_slice())
            .await
            .unwrap();

        // Sync Bola's state to get the latest
        bola_group.sync().await.unwrap();
        // Find Bola's updated list of messages
        bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
        // Bola should have been able to decrypt the last message
        assert_eq!(bola_messages.len(), 2);
        assert_eq!(
            bola_messages.get(1).unwrap().decrypted_message_bytes,
            vec![1, 2, 3]
        )
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_get_and_set_consent() {
        let bo_wallet = generate_local_wallet();
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&bo_wallet).await;
        let record = StoredConsentRecord::new(
            ConsentType::Address,
            ConsentState::Denied,
            bo_wallet.get_address(),
        );
        alix.set_consent_states(&[record]).await.unwrap();
        let inbox_consent = alix
            .get_consent_state(ConsentType::InboxId, bo.inbox_id())
            .await
            .unwrap();
        let address_consent = alix
            .get_consent_state(ConsentType::Address, bo_wallet.get_address())
            .await
            .unwrap();

        assert_eq!(inbox_consent, ConsentState::Denied);
        assert_eq!(address_consent, ConsentState::Denied);
    }

    async fn get_key_package_init_key<
        ApiClient: XmtpApi,
        Verifier: SmartContractSignatureVerifier,
    >(
        client: &Client<ApiClient, Verifier>,
        installation_id: &[u8],
    ) -> Vec<u8> {
        let kps = client
            .get_key_packages_for_installation_ids(vec![installation_id.to_vec()])
            .await
            .unwrap();
        let kp = kps.first().unwrap();

        serialize_key_package_hash_ref(&kp.inner, &client.mls_provider().unwrap()).unwrap()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_key_package_rotation() {
        let alix_wallet = generate_local_wallet();
        let bo_wallet = generate_local_wallet();
        let alix = ClientBuilder::new_test_client(&alix_wallet).await;
        let bo = ClientBuilder::new_test_client(&bo_wallet).await;
        let bo_store = bo.store();

        let alix_original_init_key =
            get_key_package_init_key(&alix, &alix.installation_public_key()).await;
        let bo_original_init_key =
            get_key_package_init_key(&bo, &bo.installation_public_key()).await;

        // Bo's original key should be deleted
        let bo_original_from_db = bo_store
            .conn()
            .unwrap()
            .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone());
        assert!(bo_original_from_db.is_ok());

        alix.create_group_with_members(
            &[bo_wallet.get_address()],
            None,
            GroupMetadataOptions::default(),
        )
        .await
        .unwrap();

        bo.sync_welcomes(&bo.store().conn().unwrap()).await.unwrap();

        let bo_new_key = get_key_package_init_key(&bo, &bo.installation_public_key()).await;
        // Bo's key should have changed
        assert_ne!(bo_original_init_key, bo_new_key);

        bo.sync_welcomes(&bo.store().conn().unwrap()).await.unwrap();
        let bo_new_key_2 = get_key_package_init_key(&bo, &bo.installation_public_key()).await;
        // Bo's key should not have changed syncing the second time.
        assert_eq!(bo_new_key, bo_new_key_2);

        alix.sync_welcomes(&alix.store().conn().unwrap())
            .await
            .unwrap();
        let alix_key_2 = get_key_package_init_key(&alix, &alix.installation_public_key()).await;
        // Alix's key should not have changed at all
        assert_eq!(alix_original_init_key, alix_key_2);

        alix.create_group_with_members(
            &[bo_wallet.get_address()],
            None,
            GroupMetadataOptions::default(),
        )
        .await
        .unwrap();
        bo.sync_welcomes(&bo.store().conn().unwrap()).await.unwrap();

        // Bo should have two groups now
        let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(bo_groups.len(), 2);

        // Bo's original key should be deleted
        let bo_original_after_delete = bo_store
            .conn()
            .unwrap()
            .find_key_package_history_entry_by_hash_ref(bo_original_init_key);
        assert!(bo_original_after_delete.is_err());
    }
}
