use xmtp_common::{RetryableError, retryable};

use crate::{ConnectionExt, XmtpMlsStorageProvider};

use bincode;
use diesel::{
    prelude::*,
    sql_types::Binary,
    {RunQueryDsl, sql_query},
};
use openmls_traits::storage::*;
use serde::Serialize;
use std::cell::RefCell;
mod transactions;
pub use transactions::XmtpMlsTransactionProvider;

const SELECT_QUERY: &str =
    "SELECT value_bytes FROM openmls_key_value WHERE key_bytes = ? AND version = ?";
const REPLACE_QUERY: &str =
    "REPLACE INTO openmls_key_value (key_bytes, version, value_bytes) VALUES (?, ?, ?)";
const UPDATE_QUERY: &str =
    "UPDATE openmls_key_value SET value_bytes = ? WHERE key_bytes = ? AND version = ?";
const DELETE_QUERY: &str = "DELETE FROM openmls_key_value WHERE key_bytes = ? AND version = ?";

#[derive(QueryableByName, Debug, Clone, PartialEq, Eq)]
#[diesel(table_name = openmls_key_value)]
struct StorageData {
    #[diesel(sql_type = Binary)]
    value_bytes: Vec<u8>,
}

pub struct SqlKeyStore<C> {
    // Directly wrap the DbConnection which is a SqliteConnection in this case
    conn: C,
}

impl<C> SqlKeyStore<C> {
    pub fn new(conn: C) -> Self {
        Self { conn }
    }

    pub fn conn(&self) -> &C {
        &self.conn
    }
}

impl<D, C> From<D> for SqlKeyStore<D>
where
    D: crate::DbQuery<C>,
    D: ConnectionExt<Connection = C>,
    C: ConnectionExt,
{
    fn from(value: D) -> Self {
        Self { conn: value }
    }
}

// refactor to use diesel directly
impl<C> SqlKeyStore<C>
where
    C: ConnectionExt,
{
    fn select_query<const VERSION: u16>(
        &self,
        storage_key: &Vec<u8>,
    ) -> Result<Vec<StorageData>, crate::ConnectionError> {
        self.conn.raw_query_read(|conn| {
            sql_query(SELECT_QUERY)
                .bind::<diesel::sql_types::Binary, _>(&storage_key)
                .bind::<diesel::sql_types::Integer, _>(VERSION as i32)
                .load(conn)
        })
    }

    fn replace_query<const VERSION: u16>(
        &self,
        storage_key: &Vec<u8>,
        value: &[u8],
    ) -> Result<usize, crate::ConnectionError> {
        self.conn.raw_query_write(|conn| {
            sql_query(REPLACE_QUERY)
                .bind::<diesel::sql_types::Binary, _>(&storage_key)
                .bind::<diesel::sql_types::Integer, _>(VERSION as i32)
                .bind::<diesel::sql_types::Binary, _>(&value)
                .execute(conn)
        })
    }

    fn update_query<const VERSION: u16>(
        &self,
        storage_key: &Vec<u8>,
        modified_data: &Vec<u8>,
    ) -> Result<usize, crate::ConnectionError> {
        self.conn.raw_query_write(|conn| {
            sql_query(UPDATE_QUERY)
                .bind::<diesel::sql_types::Binary, _>(&modified_data)
                .bind::<diesel::sql_types::Binary, _>(&storage_key)
                .bind::<diesel::sql_types::Integer, _>(VERSION as i32)
                .execute(conn)
        })
    }

    pub fn write<const VERSION: u16>(
        &self,
        label: &[u8],
        key: &[u8],
        value: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        let storage_key = build_key_from_vec::<VERSION>(label, key.to_vec());
        let _ = self.replace_query::<VERSION>(&storage_key, value)?;
        Ok(())
    }

    pub fn append<const VERSION: u16>(
        &self,
        label: &[u8],
        key: &[u8],
        value: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        tracing::trace!("append {}", String::from_utf8_lossy(label));

        let storage_key = build_key_from_vec::<VERSION>(label, key.to_vec());
        let data = self.select_query::<VERSION>(&storage_key)?;

        if let Some(entry) = data.into_iter().next() {
            // The value in the storage is an array of array of bytes
            match bincode::deserialize::<Vec<Vec<u8>>>(&entry.value_bytes) {
                Ok(mut deserialized) => {
                    deserialized.push(value.to_vec());
                    let modified_data = bincode::serialize(&deserialized)?;

                    let _ = self.update_query::<VERSION>(&storage_key, &modified_data)?;
                    Ok(())
                }
                Err(_e) => Err(SqlKeyStoreError::SerializationError),
            }
        } else {
            // Add a first entry
            let value_bytes = &bincode::serialize(&vec![value])?;
            let _ = self.replace_query::<VERSION>(&storage_key, value_bytes)?;

            Ok(())
        }
    }

    pub fn remove_item<const VERSION: u16>(
        &self,
        label: &[u8],
        key: &[u8],
        value: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        tracing::trace!("remove_item {}", String::from_utf8_lossy(label));

        let storage_key = build_key_from_vec::<VERSION>(label, key.to_vec());
        let data: Vec<StorageData> = self.select_query::<VERSION>(&storage_key)?;

        if let Some(entry) = data.into_iter().next() {
            // The value in the storage is an array of array of bytes.
            let mut deserialized = bincode::deserialize::<Vec<Vec<u8>>>(&entry.value_bytes)
                .map_err(|_| SqlKeyStoreError::SerializationError)?;
            let vpos = deserialized.iter().position(|v| v == value);

            if let Some(pos) = vpos {
                deserialized.remove(pos);
            }
            let modified_data = bincode::serialize(&deserialized)
                .map_err(|_| SqlKeyStoreError::SerializationError)?;

            let _ = self.update_query::<VERSION>(&storage_key, &modified_data)?;
            Ok(())
        } else {
            // Add a first entry
            let value_bytes =
                bincode::serialize(&[value]).map_err(|_| SqlKeyStoreError::SerializationError)?;
            let _ = self.replace_query::<VERSION>(&storage_key, &value_bytes)?;
            Ok(())
        }
    }

    pub fn read<const VERSION: u16, V: Entity<VERSION>>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<Option<V>, <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        let storage_key = build_key_from_vec::<VERSION>(label, key.to_vec());

        let data = self.select_query::<VERSION>(&storage_key)?;

        if let Some(entry) = data.into_iter().next() {
            let deserialized = bincode::deserialize::<V>(&entry.value_bytes)
                .map_err(|_| SqlKeyStoreError::SerializationError)?;

            Ok(Some(deserialized))
        } else {
            Ok(None)
        }
    }

    pub fn read_list<const VERSION: u16, V: Entity<VERSION>>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<Vec<V>, <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        let storage_key = build_key_from_vec::<VERSION>(label, key.to_vec());
        let results = self.select_query::<VERSION>(&storage_key)?;

        if let Some(entry) = results.into_iter().next() {
            let list = bincode::deserialize::<Vec<Vec<u8>>>(&entry.value_bytes)?;

            // Read the values from the bytes in the list
            let mut deserialized_list = Vec::new();
            for v in list {
                match bincode::deserialize::<V>(&v) {
                    Ok(deserialized_value) => deserialized_list.push(deserialized_value),
                    Err(e) => {
                        tracing::error!("Error occurred: {}", e);
                        return Err(SqlKeyStoreError::SerializationError);
                    }
                }
            }
            Ok(deserialized_list)
        } else {
            Ok(vec![])
        }
    }

    pub fn delete<const VERSION: u16>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        let storage_key = build_key_from_vec::<VERSION>(label, key.to_vec());
        self.conn.raw_query_write(|conn| {
            sql_query(DELETE_QUERY)
                .bind::<diesel::sql_types::Binary, _>(&storage_key)
                .bind::<diesel::sql_types::Integer, _>(VERSION as i32)
                .execute(conn)
        })?;
        Ok(())
    }
}

/// Errors thrown by the key store.
/// General error type for Mls Storage Trait
#[derive(thiserror::Error, Debug)]
pub enum SqlKeyStoreError {
    #[error("The key store does not allow storing serialized values.")]
    UnsupportedValueTypeBytes,
    #[error("Updating is not supported by this key store.")]
    UnsupportedMethod,
    #[error("Error serializing value.")]
    SerializationError,
    #[error("Value does not exist.")]
    NotFound,
    #[error("database error: {0}")]
    Storage(#[from] diesel::result::Error),
    #[error("connection {0}")]
    Connection(#[from] crate::ConnectionError),
}

impl RetryableError for SqlKeyStoreError {
    fn is_retryable(&self) -> bool {
        use SqlKeyStoreError::*;
        match self {
            Storage(err) => retryable!(err),
            SerializationError => false,
            UnsupportedMethod => false,
            UnsupportedValueTypeBytes => false,
            NotFound => false,
            Connection(c) => retryable!(c),
        }
    }
}

const KEY_PACKAGE_LABEL: &[u8] = b"KeyPackage";
const ENCRYPTION_KEY_PAIR_LABEL: &[u8] = b"EncryptionKeyPair";
const SIGNATURE_KEY_PAIR_LABEL: &[u8] = b"SignatureKeyPair";
const EPOCH_KEY_PAIRS_LABEL: &[u8] = b"EpochKeyPairs";
pub const KEY_PACKAGE_REFERENCES: &[u8] = b"KeyPackageReferences";
pub const KEY_PACKAGE_WRAPPER_PRIVATE_KEY: &[u8] = b"KeyPackageWrapperPrivateKey";

// related to PublicGroup
const TREE_LABEL: &[u8] = b"Tree";
const GROUP_CONTEXT_LABEL: &[u8] = b"GroupContext";
const INTERIM_TRANSCRIPT_HASH_LABEL: &[u8] = b"InterimTranscriptHash";
const CONFIRMATION_TAG_LABEL: &[u8] = b"ConfirmationTag";

// related to CoreGroup
const OWN_LEAF_NODE_INDEX_LABEL: &[u8] = b"OwnLeafNodeIndex";
const EPOCH_SECRETS_LABEL: &[u8] = b"EpochSecrets";
const MESSAGE_SECRETS_LABEL: &[u8] = b"MessageSecrets";

// related to MlsGroup
const JOIN_CONFIG_LABEL: &[u8] = b"MlsGroupJoinConfig";
const OWN_LEAF_NODES_LABEL: &[u8] = b"OwnLeafNodes";
const GROUP_STATE_LABEL: &[u8] = b"GroupState";
const QUEUED_PROPOSAL_LABEL: &[u8] = b"QueuedProposal";
const PROPOSAL_QUEUE_REFS_LABEL: &[u8] = b"ProposalQueueRefs";
const RESUMPTION_PSK_STORE_LABEL: &[u8] = b"ResumptionPskStore";

impl<C> StorageProvider<CURRENT_VERSION> for SqlKeyStore<C>
where
    C: ConnectionExt,
{
    type Error = SqlKeyStoreError;

    fn queue_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
        proposal: &QueuedProposal,
    ) -> Result<(), Self::Error> {
        // write proposal to key (group_id, proposal_ref)
        let key = bincode::serialize(&(group_id, proposal_ref))?;
        let value = bincode::serialize(proposal)?;
        self.write::<CURRENT_VERSION>(QUEUED_PROPOSAL_LABEL, &key, &value)?;

        // update proposal list for group_id
        let key = build_key::<CURRENT_VERSION, &GroupId>(PROPOSAL_QUEUE_REFS_LABEL, group_id)?;
        let value = bincode::serialize(proposal_ref)?;
        self.append::<CURRENT_VERSION>(PROPOSAL_QUEUE_REFS_LABEL, &key, &value)?;

        Ok(())
    }

    fn write_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        tree: &TreeSync,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(TREE_LABEL, group_id)?;
        let value = bincode::serialize(&tree)?;
        self.write::<CURRENT_VERSION>(TREE_LABEL, &key, &value)
    }

    fn write_interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        interim_transcript_hash: &InterimTranscriptHash,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(INTERIM_TRANSCRIPT_HASH_LABEL, group_id)?;
        let value = bincode::serialize(&interim_transcript_hash)?;
        let _ = self.write::<CURRENT_VERSION>(INTERIM_TRANSCRIPT_HASH_LABEL, &key, &value);

        Ok(())
    }

    fn write_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_context: &GroupContext,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(GROUP_CONTEXT_LABEL, group_id)?;
        let value = bincode::serialize(&group_context)?;

        self.write::<CURRENT_VERSION>(GROUP_CONTEXT_LABEL, &key, &value)
    }

    fn write_confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        confirmation_tag: &ConfirmationTag,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(CONFIRMATION_TAG_LABEL, group_id)?;
        let value = bincode::serialize(&confirmation_tag)?;

        self.write::<CURRENT_VERSION>(CONFIRMATION_TAG_LABEL, &key, &value)
    }

    fn write_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
        signature_key_pair: &SignatureKeyPair,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &SignaturePublicKey>(
            SIGNATURE_KEY_PAIR_LABEL,
            public_key,
        )?;
        let value = bincode::serialize(&signature_key_pair)?;

        self.write::<CURRENT_VERSION>(SIGNATURE_KEY_PAIR_LABEL, &key, &value)
    }

    fn queued_proposal_refs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<ProposalRef>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(PROPOSAL_QUEUE_REFS_LABEL, group_id)?;
        self.read_list(PROPOSAL_QUEUE_REFS_LABEL, &key)
    }

    fn queued_proposals<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<(ProposalRef, QueuedProposal)>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(PROPOSAL_QUEUE_REFS_LABEL, group_id)?;
        let refs: Vec<ProposalRef> = self.read_list(PROPOSAL_QUEUE_REFS_LABEL, &key)?;

        refs.into_iter()
            .map(|proposal_ref| -> Result<_, _> {
                let key = bincode::serialize(&(group_id, &proposal_ref))?;
                match self.read(QUEUED_PROPOSAL_LABEL, &key)? {
                    Some(proposal) => Ok((proposal_ref, proposal)),
                    None => Err(SqlKeyStoreError::NotFound),
                }
            })
            .collect::<Result<Vec<_>, _>>()
    }

    fn tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<TreeSync>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(TREE_LABEL, group_id)?;

        self.read(TREE_LABEL, &key)
    }

    fn group_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupContext>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(GROUP_CONTEXT_LABEL, group_id)?;

        self.read(GROUP_CONTEXT_LABEL, &key)
    }

    fn interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<InterimTranscriptHash>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(INTERIM_TRANSCRIPT_HASH_LABEL, group_id)?;

        self.read(INTERIM_TRANSCRIPT_HASH_LABEL, &key)
    }

    fn confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ConfirmationTag>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(CONFIRMATION_TAG_LABEL, group_id)?;

        self.read(CONFIRMATION_TAG_LABEL, &key)
    }

    fn signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<Option<SignatureKeyPair>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &SignaturePublicKey>(
            SIGNATURE_KEY_PAIR_LABEL,
            public_key,
        )?;

        self.read(SIGNATURE_KEY_PAIR_LABEL, &key)
    }

    fn write_key_package<
        HashReference: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &HashReference,
        key_package: &KeyPackage,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &HashReference>(KEY_PACKAGE_LABEL, hash_ref)?;
        let value = bincode::serialize(&key_package)?;

        // Store the key package
        self.write::<CURRENT_VERSION>(KEY_PACKAGE_LABEL, &key, &value)
    }

    fn write_psk<
        PskId: traits::PskId<CURRENT_VERSION>,
        PskBundle: traits::PskBundle<CURRENT_VERSION>,
    >(
        &self,
        _psk_id: &PskId,
        _psk: &PskBundle,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn write_encryption_key_pair<
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
        key_pair: &HpkeKeyPair,
    ) -> Result<(), Self::Error> {
        let key =
            build_key::<CURRENT_VERSION, &EncryptionKey>(ENCRYPTION_KEY_PAIR_LABEL, public_key)?;

        self.write::<CURRENT_VERSION>(
            ENCRYPTION_KEY_PAIR_LABEL,
            &key,
            &bincode::serialize(key_pair)?,
        )
    }

    fn key_package<
        HashReference: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &HashReference,
    ) -> Result<Option<KeyPackage>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &HashReference>(KEY_PACKAGE_LABEL, hash_ref)?;

        self.read(KEY_PACKAGE_LABEL, &key)
    }

    fn psk<PskBundle: traits::PskBundle<CURRENT_VERSION>, PskId: traits::PskId<CURRENT_VERSION>>(
        &self,
        _psk_id: &PskId,
    ) -> Result<Option<PskBundle>, Self::Error> {
        Ok(None)
    }

    fn encryption_key_pair<
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<Option<HpkeKeyPair>, Self::Error> {
        let key =
            build_key::<CURRENT_VERSION, &EncryptionKey>(ENCRYPTION_KEY_PAIR_LABEL, public_key)?;

        self.read(ENCRYPTION_KEY_PAIR_LABEL, &key)
    }

    fn delete_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &SignaturePublicKey>(
            SIGNATURE_KEY_PAIR_LABEL,
            public_key,
        )?;

        self.delete::<CURRENT_VERSION>(SIGNATURE_KEY_PAIR_LABEL, &key)
    }

    fn delete_encryption_key_pair<EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>>(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<(), Self::Error> {
        let key =
            build_key::<CURRENT_VERSION, &EncryptionKey>(ENCRYPTION_KEY_PAIR_LABEL, public_key)?;

        self.delete::<CURRENT_VERSION>(ENCRYPTION_KEY_PAIR_LABEL, &key)
    }

    fn delete_key_package<HashReference: traits::HashReference<CURRENT_VERSION>>(
        &self,
        hash_ref: &HashReference,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &HashReference>(KEY_PACKAGE_LABEL, hash_ref)?;
        self.delete::<CURRENT_VERSION>(KEY_PACKAGE_LABEL, &key)
    }

    fn delete_psk<PskKey: traits::PskId<CURRENT_VERSION>>(
        &self,
        _psk_id: &PskKey,
    ) -> Result<(), Self::Error> {
        Err(SqlKeyStoreError::UnsupportedMethod)
    }

    fn group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupState>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(GROUP_STATE_LABEL, group_id)?;

        self.read(GROUP_STATE_LABEL, &key)
    }

    fn write_group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_state: &GroupState,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(GROUP_STATE_LABEL, group_id)?;

        self.write::<CURRENT_VERSION>(GROUP_STATE_LABEL, &key, &bincode::serialize(group_state)?)
    }

    fn delete_group_state<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(GROUP_STATE_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(GROUP_STATE_LABEL, &key)
    }

    fn message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MessageSecrets>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(MESSAGE_SECRETS_LABEL, group_id)?;

        self.read(MESSAGE_SECRETS_LABEL, &key)
    }

    fn write_message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        message_secrets: &MessageSecrets,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(MESSAGE_SECRETS_LABEL, group_id)?;

        self.write::<CURRENT_VERSION>(
            MESSAGE_SECRETS_LABEL,
            &key,
            &bincode::serialize(message_secrets)?,
        )
    }

    fn delete_message_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(MESSAGE_SECRETS_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(MESSAGE_SECRETS_LABEL, &key)
    }

    fn resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ResumptionPskStore>, Self::Error> {
        self.read(RESUMPTION_PSK_STORE_LABEL, &bincode::serialize(group_id)?)
    }

    fn write_resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        resumption_psk_store: &ResumptionPskStore,
    ) -> Result<(), Self::Error> {
        self.write::<CURRENT_VERSION>(
            RESUMPTION_PSK_STORE_LABEL,
            &bincode::serialize(group_id)?,
            &bincode::serialize(resumption_psk_store)?,
        )
    }

    fn delete_all_resumption_psk_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.delete::<CURRENT_VERSION>(RESUMPTION_PSK_STORE_LABEL, &bincode::serialize(group_id)?)
    }

    fn own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<LeafNodeIndex>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(OWN_LEAF_NODE_INDEX_LABEL, group_id)?;
        self.read(OWN_LEAF_NODE_INDEX_LABEL, &key)
    }

    fn write_own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        own_leaf_index: &LeafNodeIndex,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(OWN_LEAF_NODE_INDEX_LABEL, group_id)?;
        self.write::<CURRENT_VERSION>(
            OWN_LEAF_NODE_INDEX_LABEL,
            &key,
            &bincode::serialize(own_leaf_index)?,
        )
    }

    fn delete_own_leaf_index<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(OWN_LEAF_NODE_INDEX_LABEL, group_id)?;
        self.delete::<CURRENT_VERSION>(OWN_LEAF_NODE_INDEX_LABEL, &key)
    }

    fn group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupEpochSecrets>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(EPOCH_SECRETS_LABEL, group_id)?;
        self.read(EPOCH_SECRETS_LABEL, &key)
    }

    fn write_group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_epoch_secrets: &GroupEpochSecrets,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(EPOCH_SECRETS_LABEL, group_id)?;
        self.write::<CURRENT_VERSION>(
            EPOCH_SECRETS_LABEL,
            &key,
            &bincode::serialize(group_epoch_secrets)?,
        )
    }

    fn delete_group_epoch_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(EPOCH_SECRETS_LABEL, group_id)?;
        self.delete::<CURRENT_VERSION>(EPOCH_SECRETS_LABEL, &key)
    }

    fn write_encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
        key_pairs: &[HpkeKeyPair],
    ) -> Result<(), Self::Error> {
        let key = epoch_key_pairs_id(group_id, epoch, leaf_index)?;
        let value = bincode::serialize(key_pairs)?;
        tracing::trace!("Writing encryption epoch key pairs");

        self.write::<CURRENT_VERSION>(EPOCH_KEY_PAIRS_LABEL, &key, &value)
    }

    fn encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
    ) -> Result<Vec<HpkeKeyPair>, Self::Error> {
        tracing::trace!("Reading encryption epoch key pairs");

        let key = epoch_key_pairs_id(group_id, epoch, leaf_index)?;
        let storage_key = build_key_from_vec::<CURRENT_VERSION>(EPOCH_KEY_PAIRS_LABEL, key);
        tracing::trace!("  key: {}", hex::encode(&storage_key));

        let query = "SELECT value_bytes FROM openmls_key_value WHERE key_bytes = ? AND version = ?";

        let data: Vec<StorageData> = self.conn.raw_query_read(|conn| {
            sql_query(query)
                .bind::<diesel::sql_types::Binary, _>(&storage_key)
                .bind::<diesel::sql_types::Integer, _>(CURRENT_VERSION as i32)
                .load(conn)
        })?;

        if let Some(entry) = data.into_iter().next() {
            match bincode::deserialize::<Vec<HpkeKeyPair>>(&entry.value_bytes) {
                Ok(deserialized) => Ok(deserialized),
                Err(e) => {
                    eprintln!("Error occurred: {}", e);
                    Err(SqlKeyStoreError::SerializationError)
                }
            }
        } else {
            Ok(vec![])
        }
    }

    fn delete_encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
    ) -> Result<(), Self::Error> {
        let key = epoch_key_pairs_id(group_id, epoch, leaf_index)?;

        self.delete::<CURRENT_VERSION>(EPOCH_KEY_PAIRS_LABEL, &key)
    }

    fn clear_proposal_queue<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(PROPOSAL_QUEUE_REFS_LABEL, group_id)?;
        let proposal_refs: Vec<ProposalRef> = self.read_list(PROPOSAL_QUEUE_REFS_LABEL, &key)?;

        for proposal_ref in proposal_refs {
            let key = bincode::serialize(&(group_id, proposal_ref))?;
            self.delete::<CURRENT_VERSION>(QUEUED_PROPOSAL_LABEL, &key)?;
        }

        let key = build_key::<CURRENT_VERSION, &GroupId>(PROPOSAL_QUEUE_REFS_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(PROPOSAL_QUEUE_REFS_LABEL, &key)
    }

    fn mls_group_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MlsGroupJoinConfig>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(JOIN_CONFIG_LABEL, group_id)?;

        self.read(JOIN_CONFIG_LABEL, &key)
    }

    fn write_mls_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        config: &MlsGroupJoinConfig,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(JOIN_CONFIG_LABEL, group_id)?;
        let value = bincode::serialize(config)?;

        self.write::<CURRENT_VERSION>(JOIN_CONFIG_LABEL, &key, &value)
    }

    fn own_leaf_nodes<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<LeafNode>, Self::Error> {
        tracing::trace!("own_leaf_nodes");
        let key = build_key::<CURRENT_VERSION, &GroupId>(OWN_LEAF_NODES_LABEL, group_id)?;

        self.read_list(OWN_LEAF_NODES_LABEL, &key)
    }

    fn append_own_leaf_node<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        leaf_node: &LeafNode,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(OWN_LEAF_NODES_LABEL, group_id)?;
        let value = bincode::serialize(leaf_node)?;

        self.append::<CURRENT_VERSION>(OWN_LEAF_NODES_LABEL, &key, &value)
    }

    fn delete_own_leaf_nodes<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(OWN_LEAF_NODES_LABEL, group_id)?;
        self.delete::<CURRENT_VERSION>(OWN_LEAF_NODES_LABEL, &key)
    }

    fn delete_group_config<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(JOIN_CONFIG_LABEL, group_id)?;
        self.delete::<CURRENT_VERSION>(JOIN_CONFIG_LABEL, &key)
    }

    fn delete_tree<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(TREE_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(TREE_LABEL, &key)
    }

    fn delete_confirmation_tag<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(CONFIRMATION_TAG_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(CONFIRMATION_TAG_LABEL, &key)
    }

    fn delete_context<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(GROUP_CONTEXT_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(GROUP_CONTEXT_LABEL, &key)
    }

    fn delete_interim_transcript_hash<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(INTERIM_TRANSCRIPT_HASH_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(INTERIM_TRANSCRIPT_HASH_LABEL, &key)
    }

    fn remove_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
    ) -> Result<(), Self::Error> {
        // Delete the proposal ref
        let key = build_key::<CURRENT_VERSION, &GroupId>(PROPOSAL_QUEUE_REFS_LABEL, group_id)?;
        let value = bincode::serialize(proposal_ref)?;
        self.remove_item::<CURRENT_VERSION>(PROPOSAL_QUEUE_REFS_LABEL, &key, &value)?;

        // Delete the proposal
        let key = bincode::serialize(&(group_id, proposal_ref))?;
        self.delete::<CURRENT_VERSION>(QUEUED_PROPOSAL_LABEL, &key)
    }
}

/// Build a key with version and label.
fn build_key_from_vec<const V: u16>(label: &[u8], key: Vec<u8>) -> Vec<u8> {
    let mut key_out = label.to_vec();
    key_out.extend_from_slice(&key);
    key_out.extend_from_slice(&u16::to_be_bytes(V));
    key_out
}

/// Build a key with version and label.
fn build_key<const V: u16, K: Serialize>(
    label: &[u8],
    key: K,
) -> Result<Vec<u8>, SqlKeyStoreError> {
    let key_vec = bincode::serialize(&key)?;
    Ok(build_key_from_vec::<V>(label, key_vec))
}

fn epoch_key_pairs_id(
    group_id: &impl traits::GroupId<CURRENT_VERSION>,
    epoch: &impl traits::EpochKey<CURRENT_VERSION>,
    leaf_index: u32,
) -> Result<Vec<u8>, SqlKeyStoreError> {
    let mut key = bincode::serialize(group_id)?;
    key.extend_from_slice(&bincode::serialize(epoch)?);
    key.extend_from_slice(&bincode::serialize(&leaf_index)?);
    Ok(key)
}

impl From<bincode::Error> for SqlKeyStoreError {
    fn from(_: bincode::Error) -> Self {
        Self::SerializationError
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use openmls::group::GroupId;
    use openmls_basic_credential::{SignatureKeyPair, StorageId};
    use openmls_traits::{
        OpenMlsProvider,
        storage::{
            CURRENT_VERSION, Entity, Key, StorageProvider,
            traits::{self},
        },
    };
    use serde::{Deserialize, Serialize};

    use super::SqlKeyStore;
    use crate::{
        XmtpTestDb, sql_key_store::SqlKeyStoreError, xmtp_openmls_provider::XmtpOpenMlsProvider,
    };
    use xmtp_cryptography::configuration::CIPHERSUITE;

    #[xmtp_common::test]
    async fn store_read_delete() {
        let store = crate::TestDb::create_persistent_store(None).await;
        let conn = store.conn();
        let key_store = SqlKeyStore::new(conn);

        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        let public_key = StorageId::from(signature_keys.to_public_vec());
        assert!(
            key_store
                .signature_key_pair::<StorageId, SignatureKeyPair>(&public_key)
                .unwrap()
                .is_none()
        );

        key_store
            .write_signature_key_pair::<StorageId, SignatureKeyPair>(&public_key, &signature_keys)
            .unwrap();

        assert!(
            key_store
                .signature_key_pair::<StorageId, SignatureKeyPair>(&public_key)
                .unwrap()
                .is_some()
        );

        key_store
            .delete_signature_key_pair::<StorageId>(&public_key)
            .unwrap();

        assert!(
            key_store
                .signature_key_pair::<StorageId, SignatureKeyPair>(&public_key)
                .unwrap()
                .is_none()
        );
    }

    #[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
    struct Proposal(Vec<u8>);
    impl traits::QueuedProposal<CURRENT_VERSION> for Proposal {}
    impl Entity<CURRENT_VERSION> for Proposal {}

    #[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy)]
    struct ProposalRef(usize);
    impl traits::ProposalRef<CURRENT_VERSION> for ProposalRef {}
    impl Key<CURRENT_VERSION> for ProposalRef {}
    impl Entity<CURRENT_VERSION> for ProposalRef {}

    #[xmtp_common::test]
    async fn list_append_remove() {
        let store = crate::TestDb::create_persistent_store(None).await;
        let conn = store.conn();
        let mls_store = SqlKeyStore::new(conn);
        let provider = XmtpOpenMlsProvider::new(mls_store);
        let group_id = GroupId::random(provider.rand());
        let proposals = (0..10)
            .map(|i| Proposal(format!("TestProposal{i}").as_bytes().to_vec()))
            .collect::<Vec<_>>();

        // Store proposals
        for (i, proposal) in proposals.iter().enumerate() {
            provider
                .storage()
                .queue_proposal::<GroupId, ProposalRef, Proposal>(
                    &group_id,
                    &ProposalRef(i),
                    proposal,
                )
                .expect("Failed to queue proposal");
        }

        tracing::trace!("Finished with queued proposals");
        // Read proposal refs
        let proposal_refs_read: Vec<ProposalRef> = provider
            .storage()
            .queued_proposal_refs(&group_id)
            .expect("Failed to read proposal refs");
        assert_eq!(
            (0..10).map(ProposalRef).collect::<Vec<_>>(),
            proposal_refs_read
        );

        // Read proposals
        let proposals_read: Vec<(ProposalRef, Proposal)> =
            provider.storage().queued_proposals(&group_id).unwrap();
        let proposals_expected: Vec<(ProposalRef, Proposal)> = (0..10)
            .map(ProposalRef)
            .zip(proposals.clone().into_iter())
            .collect();
        assert_eq!(proposals_expected, proposals_read);

        // Remove proposal 5
        provider
            .storage()
            .remove_proposal(&group_id, &ProposalRef(5))
            .unwrap();

        let proposal_refs_read: Vec<ProposalRef> =
            provider.storage().queued_proposal_refs(&group_id).unwrap();
        let mut expected = (0..10).map(ProposalRef).collect::<Vec<_>>();
        expected.remove(5);
        assert_eq!(expected, proposal_refs_read);

        let proposals_read: Vec<(ProposalRef, Proposal)> =
            provider.storage().queued_proposals(&group_id).unwrap();
        let mut proposals_expected: Vec<(ProposalRef, Proposal)> = (0..10)
            .map(ProposalRef)
            .zip(proposals.clone().into_iter())
            .collect();
        proposals_expected.remove(5);
        assert_eq!(proposals_expected, proposals_read);

        // Clear all proposals
        provider
            .storage()
            .clear_proposal_queue::<GroupId, ProposalRef>(&group_id)
            .unwrap();
        let proposal_refs_read: Result<Vec<ProposalRef>, SqlKeyStoreError> =
            provider.storage().queued_proposal_refs(&group_id);
        assert!(proposal_refs_read.unwrap().is_empty());

        let proposals_read: Result<Vec<(ProposalRef, Proposal)>, SqlKeyStoreError> =
            provider.storage().queued_proposals(&group_id);
        assert!(proposals_read.unwrap().is_empty());
    }

    #[xmtp_common::test]
    async fn group_state() {
        let store = crate::TestDb::create_persistent_store(None).await;
        let conn = store.conn();
        let store = SqlKeyStore::new(conn);
        let provider = XmtpOpenMlsProvider::new(store);

        #[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy)]
        struct GroupState(usize);
        impl traits::GroupState<CURRENT_VERSION> for GroupState {}
        impl Entity<CURRENT_VERSION> for GroupState {}

        let group_id = GroupId::random(provider.rand());

        // Group state
        provider
            .storage()
            .write_group_state(&group_id, &GroupState(77))
            .unwrap();

        // Read group state
        let group_state: Option<GroupState> = provider.storage().group_state(&group_id).unwrap();
        assert_eq!(GroupState(77), group_state.unwrap());
    }
}
