use super::encrypted_store::db_connection::DbConnection;
use diesel::{
    prelude::*,
    sql_types::Binary,
    {sql_query, RunQueryDsl},
};
use log::error;
// use openmls::prelude::tls_codec::Serialize;
use openmls_traits::storage::*;
use serde::Serialize;
use serde_json::{from_slice, from_value, Value};

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

#[derive(Debug, Clone)]
pub struct SqlKeyStore {
    // Directly wrap the DbConnection which is a SqliteConnection in this case
    conn: DbConnection,
}

impl SqlKeyStore {
    pub fn new(conn: DbConnection) -> Self {
        Self { conn }
    }

    pub fn conn(&self) -> DbConnection {
        self.conn.clone()
    }

    pub fn conn_ref(&self) -> &DbConnection {
        &self.conn
    }

    fn select_query<const VERSION: u16>(
        &self,
        storage_key: &Vec<u8>,
    ) -> Result<Vec<StorageData>, diesel::result::Error> {
        self.conn().raw_query(|conn| {
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
    ) -> Result<usize, diesel::result::Error> {
        self.conn().raw_query(|conn| {
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
    ) -> Result<usize, diesel::result::Error> {
        self.conn().raw_query(|conn| {
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
        log::debug!("write {}", String::from_utf8_lossy(label));

        let storage_key = build_key_from_vec::<VERSION>(label, key.to_vec());

        let _ = self.replace_query::<VERSION>(&storage_key, value);

        Ok(())
    }

    pub fn append<const VERSION: u16>(
        &self,
        label: &[u8],
        key: &[u8],
        value: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        log::debug!("append {}", String::from_utf8_lossy(label));

        let storage_key = build_key_from_vec::<VERSION>(label, key.to_vec());
        let current_data: Result<Vec<StorageData>, diesel::result::Error> =
            self.select_query::<VERSION>(&storage_key);

        match current_data {
            Ok(data) => {
                if let Some(entry) = data.into_iter().next() {
                    // The value in the storage is an array of array of bytes, encoded as json.
                    match from_slice::<Value>(&entry.value_bytes) {
                        Ok(mut deserialized) => {
                            // Assuming value is JSON and needs to be added to an array
                            if let Value::Array(ref mut arr) = deserialized {
                                arr.push(Value::from(value));
                            }

                            let modified_data = serde_json::to_vec(&deserialized)
                                .map_err(|_| MemoryStorageError::SerializationError)?;

                            let _ = self.update_query::<VERSION>(&storage_key, &modified_data);
                            Ok(())
                        }
                        Err(_e) => Err(MemoryStorageError::SerializationError),
                    }
                } else {
                    // Add a first entry
                    let value_bytes = &serde_json::to_vec(&[value])?;
                    let _ = self.replace_query::<VERSION>(&storage_key, value_bytes);

                    Ok(())
                }
            }
            Err(_) => Err(MemoryStorageError::None),
        }
    }

    pub fn remove_item<const VERSION: u16>(
        &self,
        label: &[u8],
        key: &[u8],
        value: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        log::debug!("remove_item {}", String::from_utf8_lossy(label));

        let storage_key = build_key_from_vec::<VERSION>(label, key.to_vec());
        let current_data: Result<Vec<StorageData>, diesel::result::Error> =
            self.select_query::<VERSION>(&storage_key);

        match current_data {
            Ok(data) => {
                if let Some(entry) = data.into_iter().next() {
                    // The value in the storage is an array of array of bytes, encoded as json.
                    match from_slice::<Value>(&entry.value_bytes) {
                        Ok(mut deserialized) => {
                            if let Value::Array(ref mut arr) = deserialized {
                                // Find and remove the value.
                                let vpos = arr.iter().position(|v| {
                                    match from_value::<Vec<u8>>(v.clone()) {
                                        Ok(deserialized_value) => deserialized_value == value,
                                        Err(_) => false,
                                    }
                                });

                                if let Some(pos) = vpos {
                                    arr.remove(pos);
                                }
                            }
                            let modified_data = serde_json::to_vec(&deserialized)
                                .map_err(|_| MemoryStorageError::SerializationError)?;

                            let _ = self.update_query::<VERSION>(&storage_key, &modified_data);
                            Ok(())
                        }
                        Err(_) => Err(MemoryStorageError::SerializationError),
                    }
                } else {
                    // Add a first entry
                    let value_bytes = serde_json::to_vec(&[value])
                        .map_err(|_| MemoryStorageError::SerializationError)?;
                    let _ = self.replace_query::<VERSION>(&storage_key, &value_bytes);
                    Ok(())
                }
            }
            Err(_) => Err(MemoryStorageError::None),
        }
    }

    pub fn read<const VERSION: u16>(
        &self,
        label: &[u8],
        key: &[u8],
        // ) -> Result<Option<V>, <Self as StorageProvider<CURRENT_VERSION>>::Error> {
    ) -> Result<Option<Vec<u8>>, <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        log::debug!("read {}", String::from_utf8_lossy(label));

        let storage_key = build_key_from_vec::<VERSION>(label, key.to_vec());

        let results: Result<Vec<StorageData>, diesel::result::Error> =
            self.select_query::<VERSION>(&storage_key);

        match results {
            Ok(data) => {
                if let Some(entry) = data.into_iter().next() {
                    // TODO: replace with a custom/derived deserialization method
                    // match serde_json::from_slice::<V>(&entry.value_bytes) {
                    match serde_json::from_slice::<Vec<u8>>(&entry.value_bytes) {
                        Ok(deserialized) => Ok(Some(deserialized)),
                        Err(e) => {
                            eprintln!("Error occurred: {}", e);
                            Err(MemoryStorageError::SerializationError)
                        }
                    }
                } else {
                    Ok(None)
                }
            }
            Err(_e) => Err(MemoryStorageError::None),
        }
    }

    pub fn read_list<const VERSION: u16, V: Entity<VERSION>>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<Vec<V>, <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        log::debug!("read_list {}", String::from_utf8_lossy(label));

        let storage_key = build_key_from_vec::<VERSION>(label, key.to_vec());

        match self.select_query::<VERSION>(&storage_key) {
            Ok(results) => {
                if let Some(entry) = results.into_iter().next() {
                    // TODO: replace with a custom/derived deserialization method
                    let list = from_slice::<Vec<Vec<u8>>>(&entry.value_bytes)?;

                    // Read the values from the bytes in the list
                    let mut deserialized_list = Vec::new();
                    for v in list {
                        // TODO: replace with a custom/derived deserialization method
                        match serde_json::from_slice(&v) {
                            Ok(deserialized_value) => deserialized_list.push(deserialized_value),
                            Err(_) => return Err(MemoryStorageError::SerializationError),
                        }
                    }
                    Ok(deserialized_list)
                } else {
                    Ok(vec![])
                }
            }
            Err(_e) => Err(MemoryStorageError::None),
        }
    }

    pub fn delete<const VERSION: u16>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        let storage_key = build_key_from_vec::<VERSION>(label, key.to_vec());

        let _ = self.conn().raw_query(|conn| {
            sql_query(DELETE_QUERY)
                .bind::<diesel::sql_types::Binary, _>(&storage_key)
                .bind::<diesel::sql_types::Integer, _>(VERSION as i32)
                .execute(conn)
        });
        Ok(())
    }
}

/// Errors thrown by the key store.
#[derive(thiserror::Error, Debug, Copy, Clone, PartialEq, Eq)]
pub enum MemoryStorageError {
    #[error("The key store does not allow storing serialized values.")]
    UnsupportedValueTypeBytes,
    #[error("Updating is not supported by this key store.")]
    UnsupportedMethod,
    #[error("Error serializing value.")]
    SerializationError,
    #[error("Value does not exist.")]
    None,
}

const KEY_PACKAGE_LABEL: &[u8] = b"KeyPackage";
const ENCRYPTION_KEY_PAIR_LABEL: &[u8] = b"EncryptionKeyPair";
const SIGNATURE_KEY_PAIR_LABEL: &[u8] = b"SignatureKeyPair";
const EPOCH_KEY_PAIRS_LABEL: &[u8] = b"EpochKeyPairs";
pub const KEY_PACKAGE_REFERENCES: &[u8] = b"KeyPackageReferences";

// related to PublicGroup
const TREE_LABEL: &[u8] = b"Tree";
const GROUP_CONTEXT_LABEL: &[u8] = b"GroupContext";
const INTERIM_TRANSCRIPT_HASH_LABEL: &[u8] = b"InterimTranscriptHash";
const CONFIRMATION_TAG_LABEL: &[u8] = b"ConfirmationTag";

// related to CoreGroup
const OWN_LEAF_NODE_INDEX_LABEL: &[u8] = b"OwnLeafNodeIndex";
const EPOCH_SECRETS_LABEL: &[u8] = b"EpochSecrets";
const MESSAGE_SECRETS_LABEL: &[u8] = b"MessageSecrets";
const USE_RATCHET_TREE_LABEL: &[u8] = b"UseRatchetTree";

// related to MlsGroup
const JOIN_CONFIG_LABEL: &[u8] = b"MlsGroupJoinConfig";
const OWN_LEAF_NODES_LABEL: &[u8] = b"OwnLeafNodes";
const AAD_LABEL: &[u8] = b"AAD";
const GROUP_STATE_LABEL: &[u8] = b"GroupState";
const QUEUED_PROPOSAL_LABEL: &[u8] = b"QueuedProposal";
const PROPOSAL_QUEUE_REFS_LABEL: &[u8] = b"ProposalQueueRefs";
const RESUMPTION_PSK_STORE_LABEL: &[u8] = b"ResumptionPskStore";

impl StorageProvider<CURRENT_VERSION> for SqlKeyStore {
    type Error = MemoryStorageError;

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
        let key = serde_json::to_vec(&(group_id, proposal_ref))?;
        let value = serde_json::to_vec(proposal)?;
        self.write::<CURRENT_VERSION>(QUEUED_PROPOSAL_LABEL, &key, &value)?;

        // update proposal list for group_id
        let key = build_key::<CURRENT_VERSION, &GroupId>(PROPOSAL_QUEUE_REFS_LABEL, group_id)?;
        let value = serde_json::to_vec(proposal_ref)?;
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
        let value = serde_json::to_vec(&tree)?;
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
        let value = serde_json::to_vec(&interim_transcript_hash)?;
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
        let value = serde_json::to_vec(&group_context)?;
        let _ = self.write::<CURRENT_VERSION>(GROUP_CONTEXT_LABEL, &key, &value);

        Ok(())
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
        let value = serde_json::to_vec(&confirmation_tag)?;
        let _ = self.write::<CURRENT_VERSION>(CONFIRMATION_TAG_LABEL, &key, &value);

        Ok(())
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
        let value = serde_json::to_vec(&signature_key_pair)?;
        let _ = self.write::<CURRENT_VERSION>(SIGNATURE_KEY_PAIR_LABEL, &key, &value);

        Ok(())
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
                let key = serde_json::to_vec(&(group_id, &proposal_ref))?;
                match self.read::<CURRENT_VERSION>(QUEUED_PROPOSAL_LABEL, &key)? {
                    Some(proposal) => {
                        let proposal = serde_json::from_slice(&proposal)?;
                        Ok((proposal_ref, proposal))
                    }
                    None => Err(MemoryStorageError::SerializationError),
                }
            })
            .collect::<Result<Vec<_>, _>>()
    }

    fn treesync<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<TreeSync>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(TREE_LABEL, group_id)?;
        self.read::<CURRENT_VERSION>(TREE_LABEL, &key)
    }

    fn group_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupContext>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(GROUP_CONTEXT_LABEL, group_id)?;

        match self.read::<CURRENT_VERSION>(GROUP_CONTEXT_LABEL, &key) {
            Ok(Some(value)) => Ok(Some(serde_json::from_slice(&value)?)),
            Ok(None) => Ok(None),
            Err(e) => {
                error!("Error reading group_context: {:?}", e);
                Err(MemoryStorageError::None)
            }
        }
    }

    fn interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<InterimTranscriptHash>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(INTERIM_TRANSCRIPT_HASH_LABEL, group_id)?;

        match self.read::<CURRENT_VERSION>(INTERIM_TRANSCRIPT_HASH_LABEL, &key) {
            Ok(Some(value)) => Ok(Some(serde_json::from_slice(&value)?)),
            Ok(None) => Ok(None),
            Err(e) => {
                error!("Error reading interim_transcript_hash: {:?}", e);
                Err(MemoryStorageError::None)
            }
        }
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
        let value = serde_json::to_vec(&key_package)?;

        // Store the key package
        self.write::<CURRENT_VERSION>(KEY_PACKAGE_LABEL, &key, &value)?;

        Ok(())
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
            &serde_json::to_vec(key_pair)?,
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
        match self.read::<CURRENT_VERSION>(ENCRYPTION_KEY_PAIR_LABEL, &key) {
            Ok(Some(value)) => Ok(Some(serde_json::from_slice(&value)?)),
            Ok(None) => Ok(None),
            Err(e) => {
                error!("Error reading encryption_key_pair: {:?}", e);
                Err(MemoryStorageError::None)
            }
        }
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
        Err(MemoryStorageError::UnsupportedMethod)
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
        self.write::<CURRENT_VERSION>(GROUP_STATE_LABEL, &key, &serde_json::to_vec(group_state)?)
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
            &serde_json::to_vec(message_secrets)?,
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
        self.read(RESUMPTION_PSK_STORE_LABEL, &serde_json::to_vec(group_id)?)
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
            &serde_json::to_vec(group_id)?,
            &serde_json::to_vec(resumption_psk_store)?,
        )
    }

    fn delete_all_resumption_psk_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.delete::<CURRENT_VERSION>(RESUMPTION_PSK_STORE_LABEL, &serde_json::to_vec(group_id)?)
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
            &serde_json::to_vec(own_leaf_index)?,
        )
    }

    fn delete_own_leaf_index<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(OWN_LEAF_NODE_INDEX_LABEL, group_id)?;
        self.delete::<CURRENT_VERSION>(OWN_LEAF_NODE_INDEX_LABEL, &key)
    }

    fn use_ratchet_tree_extension<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<bool>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(USE_RATCHET_TREE_LABEL, group_id)?;
        match self.read::<CURRENT_VERSION>(USE_RATCHET_TREE_LABEL, &key) {
            Ok(Some(value)) => Ok(Some(serde_json::from_slice(&value)?)),
            Ok(None) => Ok(None),
            Err(e) => {
                error!("Error reading use_ratchet_tree_extension: {:?}", e);
                Err(MemoryStorageError::None)
            }
        }
    }

    fn set_use_ratchet_tree_extension<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
        value: bool,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(USE_RATCHET_TREE_LABEL, group_id)?;
        self.write::<CURRENT_VERSION>(USE_RATCHET_TREE_LABEL, &key, &serde_json::to_vec(&value)?)
    }

    fn delete_use_ratchet_tree_extension<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(USE_RATCHET_TREE_LABEL, group_id)?;
        self.delete::<CURRENT_VERSION>(USE_RATCHET_TREE_LABEL, &key)
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
            &serde_json::to_vec(group_epoch_secrets)?,
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
        let value = serde_json::to_vec(key_pairs)?;
        log::debug!("Writing encryption epoch key pairs");
        log::debug!("  key: {}", hex::encode(&key));
        log::debug!("  value: {}", hex::encode(&value));

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
        log::debug!("Reading encryption epoch key pairs");

        let key = epoch_key_pairs_id(group_id, epoch, leaf_index)?;
        let storage_key = build_key_from_vec::<CURRENT_VERSION>(EPOCH_KEY_PAIRS_LABEL, key);
        log::debug!("  key: {}", hex::encode(&storage_key));

        let query = "SELECT value_bytes FROM openmls_key_value WHERE key_bytes = ? AND version = ?";

        let results: Result<Vec<StorageData>, diesel::result::Error> =
            self.conn().raw_query(|conn| {
                sql_query(query)
                    .bind::<diesel::sql_types::Binary, _>(&storage_key)
                    .bind::<diesel::sql_types::Integer, _>(CURRENT_VERSION as i32)
                    .load(conn)
            });

        match results {
            Ok(data) => {
                if let Some(entry) = data.into_iter().next() {
                    match serde_json::from_slice::<Vec<HpkeKeyPair>>(&entry.value_bytes) {
                        Ok(deserialized) => Ok(deserialized),
                        Err(e) => {
                            eprintln!("Error occurred: {}", e);
                            Err(MemoryStorageError::SerializationError)
                        }
                    }
                } else {
                    Ok(vec![])
                }
            }
            Err(_e) => Err(MemoryStorageError::None),
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
            let key = serde_json::to_vec(&(group_id, proposal_ref))?;
            let _ = self.delete::<CURRENT_VERSION>(QUEUED_PROPOSAL_LABEL, &key);
        }

        let key = build_key::<CURRENT_VERSION, &GroupId>(PROPOSAL_QUEUE_REFS_LABEL, group_id)?;
        let _ = self.delete::<CURRENT_VERSION>(PROPOSAL_QUEUE_REFS_LABEL, &key);

        Ok(())
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
        let value = serde_json::to_vec(config)?;

        self.write::<CURRENT_VERSION>(JOIN_CONFIG_LABEL, &key, &value)
    }

    fn own_leaf_nodes<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<LeafNode>, Self::Error> {
        log::debug!("own_leaf_nodes");
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
        let value = serde_json::to_vec(leaf_node)?;
        self.append::<CURRENT_VERSION>(OWN_LEAF_NODES_LABEL, &key, &value)
    }

    fn clear_own_leaf_nodes<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(OWN_LEAF_NODES_LABEL, group_id)?;
        self.delete::<CURRENT_VERSION>(OWN_LEAF_NODES_LABEL, &key)
    }

    fn aad<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<u8>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(AAD_LABEL, group_id)?;
        match self.read::<CURRENT_VERSION>(AAD_LABEL, &key) {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    fn write_aad<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
        aad: &[u8],
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(AAD_LABEL, group_id)?;
        let value = serde_json::to_vec(&aad)?;

        self.write::<CURRENT_VERSION>(AAD_LABEL, &key, &value)
    }

    fn delete_aad<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(AAD_LABEL, group_id)?;
        self.delete::<CURRENT_VERSION>(AAD_LABEL, &key)
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
        let value = serde_json::to_vec(proposal_ref)?;
        self.remove_item::<CURRENT_VERSION>(PROPOSAL_QUEUE_REFS_LABEL, &key, &value)?;

        // Delete the proposal
        let key = serde_json::to_vec(&(group_id, proposal_ref))?;
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

/// Build a key with version and label, using a custom tls serializer.
fn build_key<const V: u16, K: Serialize>(
    label: &[u8],
    key: K,
) -> Result<Vec<u8>, MemoryStorageError> {
    let key_vec = serde_json::to_vec(&key)?;
    // let key_vec = our_tls_serializer::to_vec(&key)?;
    Ok(build_key_from_vec::<V>(label, key_vec))
}

fn epoch_key_pairs_id(
    group_id: &impl traits::GroupId<CURRENT_VERSION>,
    epoch: &impl traits::EpochKey<CURRENT_VERSION>,
    leaf_index: u32,
) -> Result<Vec<u8>, MemoryStorageError> {
    let mut key = serde_json::to_vec(group_id)?;
    key.extend_from_slice(&serde_json::to_vec(epoch)?);
    key.extend_from_slice(&serde_json::to_vec(&leaf_index)?);
    Ok(key)
}

impl From<serde_json::Error> for MemoryStorageError {
    fn from(_: serde_json::Error) -> Self {
        Self::SerializationError
    }
}

#[cfg(test)]
mod tests {
    use openmls::group::GroupId;
    use openmls_basic_credential::{SignatureKeyPair, StorageId};
    use openmls_traits::{
        storage::{traits, Entity, Key, StorageProvider, CURRENT_VERSION},
        OpenMlsProvider,
    };
    use serde::{Deserialize, Serialize};

    use super::SqlKeyStore;
    use crate::{
        configuration::CIPHERSUITE,
        storage::{sql_key_store::MemoryStorageError, EncryptedMessageStore, StorageOption},
        utils::test::tmp_path,
        xmtp_openmls_provider::XmtpOpenMlsProvider,
    };

    #[test]
    fn store_read_delete() {
        let db_path = tmp_path();
        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(db_path),
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();

        let conn = &store.conn().unwrap();
        let key_store = SqlKeyStore::new(conn.clone());

        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        let public_key = StorageId::from(signature_keys.to_public_vec());
        assert!(key_store
            .signature_key_pair::<StorageId, SignatureKeyPair>(&public_key)
            .unwrap()
            .is_none());

        key_store
            .write_signature_key_pair::<StorageId, SignatureKeyPair>(&public_key, &signature_keys)
            .unwrap();

        assert!(key_store
            .signature_key_pair::<StorageId, SignatureKeyPair>(&public_key)
            .unwrap()
            .is_some());

        key_store
            .delete_signature_key_pair::<StorageId>(&public_key)
            .unwrap();

        assert!(key_store
            .signature_key_pair::<StorageId, SignatureKeyPair>(&public_key)
            .unwrap()
            .is_none());
    }

    #[test]
    fn list_write_remove() {
        let db_path = tmp_path();
        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(db_path),
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();
        let conn = store.conn().unwrap();
        let key_store = SqlKeyStore::new(conn.clone());
        let provider = XmtpOpenMlsProvider::new(conn);
        let group_id = GroupId::random(provider.rand());

        assert!(key_store.aad::<GroupId>(&group_id).unwrap().is_empty());

        key_store
            .write_aad::<GroupId>(&group_id, "test".as_bytes())
            .unwrap();

        assert!(!key_store.aad::<GroupId>(&group_id).unwrap().is_empty());

        key_store.delete_aad::<GroupId>(&group_id).unwrap();

        assert!(key_store.aad::<GroupId>(&group_id).unwrap().is_empty());
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

    #[tokio::test]
    async fn list_append_remove() {
        let db_path = tmp_path();
        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(db_path),
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();
        let conn = store.conn().unwrap();
        let key_store = SqlKeyStore::new(conn.clone());
        let provider = XmtpOpenMlsProvider::new(conn);
        let group_id = GroupId::random(provider.rand());
        let proposals = (0..10)
            .map(|i| Proposal(format!("TestProposal{i}").as_bytes().to_vec()))
            .collect::<Vec<_>>();

        // Store proposals
        for (i, proposal) in proposals.iter().enumerate() {
            key_store
                .queue_proposal::<GroupId, ProposalRef, Proposal>(
                    &group_id,
                    &ProposalRef(i),
                    proposal,
                )
                .unwrap();
        }

        // Read proposal refs
        let proposal_refs_read: Vec<ProposalRef> =
            key_store.queued_proposal_refs(&group_id).unwrap();
        assert_eq!(
            (0..10).map(ProposalRef).collect::<Vec<_>>(),
            proposal_refs_read
        );

        // Read proposals
        let proposals_read: Vec<(ProposalRef, Proposal)> =
            key_store.queued_proposals(&group_id).unwrap();
        let proposals_expected: Vec<(ProposalRef, Proposal)> = (0..10)
            .map(ProposalRef)
            .zip(proposals.clone().into_iter())
            .collect();
        assert_eq!(proposals_expected, proposals_read);

        // Remove proposal 5
        key_store
            .remove_proposal(&group_id, &ProposalRef(5))
            .unwrap();

        let proposal_refs_read: Vec<ProposalRef> =
            key_store.queued_proposal_refs(&group_id).unwrap();
        let mut expected = (0..10).map(ProposalRef).collect::<Vec<_>>();
        expected.remove(5);
        assert_eq!(expected, proposal_refs_read);

        let proposals_read: Vec<(ProposalRef, Proposal)> =
            key_store.queued_proposals(&group_id).unwrap();
        let mut proposals_expected: Vec<(ProposalRef, Proposal)> = (0..10)
            .map(ProposalRef)
            .zip(proposals.clone().into_iter())
            .collect();
        proposals_expected.remove(5);
        assert_eq!(proposals_expected, proposals_read);

        // Clear all proposals
        key_store
            .clear_proposal_queue::<GroupId, ProposalRef>(&group_id)
            .unwrap();
        let proposal_refs_read: Result<Vec<ProposalRef>, MemoryStorageError> =
            key_store.queued_proposal_refs(&group_id);
        assert!(proposal_refs_read.unwrap().is_empty());

        let proposals_read: Result<Vec<(ProposalRef, Proposal)>, MemoryStorageError> =
            key_store.queued_proposals(&group_id);
        assert!(proposals_read.unwrap().is_empty());
    }

    #[tokio::test]
    async fn group_state() {
        let db_path = tmp_path();
        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(db_path),
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();
        let conn = store.conn().unwrap();
        let key_store = SqlKeyStore::new(conn.clone());
        let provider = XmtpOpenMlsProvider::new(conn);

        #[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy)]
        struct GroupState(usize);
        impl traits::GroupState<CURRENT_VERSION> for GroupState {}
        impl Entity<CURRENT_VERSION> for GroupState {}

        let group_id = GroupId::random(provider.rand());

        // Group state
        key_store
            .write_group_state(&group_id, &GroupState(77))
            .unwrap();

        // Read group state
        let group_state: Option<GroupState> = key_store.group_state(&group_id).unwrap();
        assert_eq!(GroupState(77), group_state.unwrap());
    }
}
