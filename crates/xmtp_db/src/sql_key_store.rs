use xmtp_common::{ErrorCode, RetryableError, retryable};

use self::transactions::MutableTransactionConnection;
use crate::{ConnectionExt, TransactionalKeyStore, XmtpMlsStorageProvider};

use self::codec::{decode_list, decode_value, encode_key, encode_list, encode_value};
use diesel::{
    prelude::*,
    sql_types::Binary,
    {RunQueryDsl, sql_query},
};
use openmls_traits::storage::*;
use serde::Serialize;
use std::collections::HashMap;
use xmtp_configuration::OPENMLS_KV_TARGET;
use xmtp_cryptography::secret::Zeroizing;

pub mod codec;
#[cfg(any(feature = "test-utils", test))]
pub mod mock;
mod transactions;

const SELECT_QUERY: &str =
    "SELECT value_bytes FROM openmls_key_value WHERE key_bytes = ? AND version = ?";
const GROUP_CONTEXT_BATCH_SIZE: usize = 500;
const REPLACE_QUERY: &str =
    "REPLACE INTO openmls_key_value (key_bytes, version, value_bytes) VALUES (?, ?, ?)";
const UPDATE_QUERY: &str =
    "UPDATE openmls_key_value SET value_bytes = ? WHERE key_bytes = ? AND version = ?";
const DELETE_QUERY: &str = "DELETE FROM openmls_key_value WHERE key_bytes = ? AND version = ?";

#[cfg(feature = "test-utils")]
#[derive(
    Selectable, Queryable, QueryableByName, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[diesel(table_name = crate::schema::openmls_key_value)]
pub struct OpenMlsKeyValue {
    pub version: i32,
    pub key_bytes: Vec<u8>,
    pub value_bytes: Vec<u8>,
}

#[cfg(feature = "test-utils")]
impl OpenMlsKeyValue {
    pub fn hash_all(conn: &mut SqliteConnection) -> Result<Vec<u8>, diesel::result::Error> {
        use crate::schema::openmls_key_value;
        use xmtp_common::Sha2Digest;
        let values = openmls_key_value::table
            .order(openmls_key_value::version.asc())
            .order(openmls_key_value::key_bytes.asc())
            .load_iter::<OpenMlsKeyValue, _>(conn)?;

        let mut hasher = xmtp_common::Sha256Digest::new();
        for (i, result) in values.enumerate() {
            let value = result?;
            hasher.update(b"version");
            hasher.update(value.version.to_be_bytes());
            hasher.update(b"key_bytes");
            hasher.update(&value.key_bytes);
            hasher.update(b"value_bytes");
            hasher.update(&value.value_bytes);
            hasher.update(b"index");
            hasher.update(i.to_be_bytes());
            hasher.update(b"\n");
        }
        Ok(hasher.finalize().to_vec())
    }
}

/// One stored value. Some values hold private keys, so the buffer is wiped on
/// drop, whatever the label. It has no `Debug`.
#[derive(QueryableByName)]
#[diesel(table_name = openmls_key_value)]
struct StorageData {
    #[diesel(sql_type = Binary, deserialize_as = Vec<u8>)]
    value_bytes: Zeroizing<Vec<u8>>,
}

/// One `read_group_contexts` row. The value buffer is wiped on drop, like
/// every key store value.
#[derive(Queryable)]
struct KeyValueRow {
    key_bytes: Vec<u8>,
    #[diesel(deserialize_as = Vec<u8>)]
    value_bytes: Zeroizing<Vec<u8>>,
}

impl TransactionalKeyStore for diesel::SqliteConnection {
    type Store<'a>
        = SqlKeyStore<MutableTransactionConnection<'a>>
    where
        Self: 'a;

    fn key_store<'a>(&'a mut self) -> Self::Store<'a> {
        SqlKeyStore::new_transactional(self)
    }
}

#[derive(Clone)]
pub struct SqlKeyStore<T> {
    // Directly wrap the DbConnection which is a SqliteConnection in this case
    conn: T,
}

// Test-only instrumentation (compiled out of release/production builds): counts
// openmls KV read round-trips (`select_query`) so the metadata
// read-amplification fix can be measured. Every `read`/`read_list`/
// `group_context` bottoms out in `select_query`, so this is the single point
// that observes an actual storage read.
//
// The counter is a **tokio task-local**, established for the duration of a
// [`count_kv_reads`] call. Being task-scoped (not thread- or process-scoped) it
// follows the measured work across `.await` points and across worker threads on
// a multi-threaded runtime, and stays isolated from any other concurrently
// running task. Reads performed outside a `count_kv_reads` scope — production
// code and unrelated tests — are ignored, so the measurement is deterministic
// regardless of runtime flavor or whether the measured accessors are sync or
// async.
// Native-only: the sole consumer is the native metadata read-amplification test,
// and the task-local relies on the (native-only) `tokio` dep enabled by
// `test-utils`.
#[cfg(all(any(test, feature = "test-utils"), not(target_arch = "wasm32")))]
tokio::task_local! {
    static KV_READS: std::cell::Cell<u64>;
}

/// Record one KV read round-trip against the enclosing [`count_kv_reads`] scope.
/// Outside such a scope this is a no-op.
#[cfg(all(any(test, feature = "test-utils"), not(target_arch = "wasm32")))]
fn record_kv_read() {
    let _ = KV_READS.try_with(|c| c.set(c.get() + 1));
}

/// Run `f` with a fresh KV-read counter scoped to the current task and return
/// its result together with the number of `select_query` round-trips it made.
#[cfg(all(any(test, feature = "test-utils"), not(target_arch = "wasm32")))]
pub fn count_kv_reads<R>(f: impl FnOnce() -> R) -> (R, u64) {
    KV_READS.sync_scope(std::cell::Cell::new(0), || {
        let result = f();
        let count = KV_READS.with(|c| c.get());
        (result, count)
    })
}

impl<A> SqlKeyStore<A> {
    pub fn new(conn: A) -> Self {
        Self { conn }
    }
}

impl<'a> SqlKeyStore<SqliteConnection> {
    pub fn new_transactional(
        conn: &'a mut SqliteConnection,
    ) -> SqlKeyStore<MutableTransactionConnection<'a>> {
        SqlKeyStore {
            conn: MutableTransactionConnection::new(conn),
        }
    }
}

// refactor to use diesel directly
impl<C> SqlKeyStore<C>
where
    C: ConnectionExt,
{
    /// Read group contexts with one query per batch for conversation lists.
    pub fn read_group_contexts(
        &self,
        group_ids: &[xmtp_proto::types::GroupId],
    ) -> Result<HashMap<xmtp_proto::types::GroupId, openmls::group::GroupContext>, SqlKeyStoreError>
    {
        use crate::schema::openmls_key_value::dsl as kv;

        let keys = group_ids
            .iter()
            .map(|id| {
                let key = build_key::<CURRENT_VERSION, &openmls::group::GroupId>(
                    GROUP_CONTEXT_LABEL,
                    &id.to_openmls(),
                )?;
                Ok::<_, SqlKeyStoreError>((
                    build_key_from_vec::<CURRENT_VERSION>(GROUP_CONTEXT_LABEL, key),
                    *id,
                ))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let mut contexts = HashMap::with_capacity(keys.len());
        let storage_keys = keys.keys().cloned().collect::<Vec<_>>();
        for batch in storage_keys.chunks(GROUP_CONTEXT_BATCH_SIZE) {
            #[cfg(all(any(test, feature = "test-utils"), not(target_arch = "wasm32")))]
            record_kv_read();
            let rows: Vec<KeyValueRow> = self.conn.raw_query(|conn| {
                kv::openmls_key_value
                    .filter(kv::key_bytes.eq_any(batch))
                    .filter(kv::version.eq(CURRENT_VERSION as i32))
                    .select((kv::key_bytes, kv::value_bytes))
                    .load(conn)
            })?;
            for row in rows {
                let id = keys[&row.key_bytes];
                contexts.insert(id, decode_value(GROUP_CONTEXT_LABEL, &row.value_bytes)?);
            }
        }
        Ok(contexts)
    }

    fn select_query<const VERSION: u16>(
        &self,
        storage_key: &Vec<u8>,
    ) -> Result<Vec<StorageData>, crate::ConnectionError> {
        #[cfg(all(any(test, feature = "test-utils"), not(target_arch = "wasm32")))]
        record_kv_read();
        self.conn.raw_query(|conn| {
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
        self.conn.raw_query(|conn| {
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
        self.conn.raw_query(|conn| {
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
            let mut list = decode_list(label, &entry.value_bytes)?;
            list.push(value.to_vec());
            let modified_data = encode_list(label, &list)?;
            let _ = self.update_query::<VERSION>(&storage_key, &modified_data)?;
            Ok(())
        } else {
            // Add a first entry
            let value_bytes = encode_list(label, &[value])?;
            let _ = self.replace_query::<VERSION>(&storage_key, &value_bytes)?;

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
            let mut list = decode_list(label, &entry.value_bytes)?;
            let vpos = list.iter().position(|v| v == value);

            if let Some(pos) = vpos {
                // The removed element is wiped as well.
                drop(Zeroizing::new(list.remove(pos)));
            }
            let modified_data = encode_list(label, &list)?;

            let _ = self.update_query::<VERSION>(&storage_key, &modified_data)?;
            Ok(())
        } else {
            // Add a first entry. A one-item array encodes as a tuple, with no
            // list length, so this row does not decode as a list. Kept
            // byte-for-byte until the value codec change.
            let value_bytes = encode_value(label, &[value])?;
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
            let deserialized = decode_value::<V>(label, &entry.value_bytes)?;

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
            let list = decode_list(label, &entry.value_bytes)?;

            // Read the values from the bytes in the list
            let mut deserialized_list = Vec::with_capacity(list.len());
            for v in list.iter() {
                deserialized_list.push(decode_value::<V>(label, v)?);
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
        self.conn.raw_query(|conn| {
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
#[derive(thiserror::Error, Debug, ErrorCode)]
pub enum SqlKeyStoreError {
    /// Unsupported value type.
    ///
    /// Key store does not allow storing serialized values. Not retryable.
    #[error("The key store does not allow storing serialized values.")]
    UnsupportedValueTypeBytes,
    /// Unsupported method.
    ///
    /// PSK operations not supported by this key store. Not retryable.
    #[error("Updating is not supported by this key store.")]
    UnsupportedMethod,
    /// Serialization error.
    ///
    /// Failed to serialize value for key store. Not retryable.
    #[error("Error serializing value.")]
    SerializationError,
    /// Value not found.
    ///
    /// Requested key not in OpenMLS key store. Not retryable.
    #[error("Value does not exist.")]
    NotFound,
    /// Database error.
    ///
    /// Underlying Diesel database error. May be retryable.
    #[error("database error: {0}")]
    Storage(#[from] diesel::result::Error),
    /// Connection error.
    ///
    /// Database connection error. Retryable.
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
pub const COMMIT_LOG_SIGNER_PRIVATE_KEY: &[u8] = b"CommitLogSignerPrivateKey";

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

// related to ApplicationExportTree
const APPLICATION_EXPORT_TREE_LABEL: &[u8] = b"ApplicationExportTree";

impl<C> StorageProvider<CURRENT_VERSION> for SqlKeyStore<C>
where
    C: ConnectionExt,
{
    type Error = SqlKeyStoreError;

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id), proposal_ref = %hex_kv(proposal_ref)), err)]
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
        let key = encode_key(QUEUED_PROPOSAL_LABEL, &(group_id, proposal_ref))?;
        let value = encode_value(QUEUED_PROPOSAL_LABEL, proposal)?;
        self.write::<CURRENT_VERSION>(QUEUED_PROPOSAL_LABEL, &key, &value)?;

        // update proposal list for group_id
        let key = build_key::<CURRENT_VERSION, &GroupId>(PROPOSAL_QUEUE_REFS_LABEL, group_id)?;
        let value = encode_value(PROPOSAL_QUEUE_REFS_LABEL, proposal_ref)?;
        self.append::<CURRENT_VERSION>(PROPOSAL_QUEUE_REFS_LABEL, &key, &value)?;

        Ok(())
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn write_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        tree: &TreeSync,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(TREE_LABEL, group_id)?;
        let value = encode_value(TREE_LABEL, tree)?;
        self.write::<CURRENT_VERSION>(TREE_LABEL, &key, &value)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn write_interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        interim_transcript_hash: &InterimTranscriptHash,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(INTERIM_TRANSCRIPT_HASH_LABEL, group_id)?;
        let value = encode_value(INTERIM_TRANSCRIPT_HASH_LABEL, interim_transcript_hash)?;
        let _ = self.write::<CURRENT_VERSION>(INTERIM_TRANSCRIPT_HASH_LABEL, &key, &value);

        Ok(())
    }

    // The group context is not public: its AppData dictionary holds the
    // commit-log signer's private key. Record only the group id.
    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn write_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_context: &GroupContext,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(GROUP_CONTEXT_LABEL, group_id)?;
        let value = encode_value(GROUP_CONTEXT_LABEL, group_context)?;

        self.write::<CURRENT_VERSION>(GROUP_CONTEXT_LABEL, &key, &value)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn write_confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        confirmation_tag: &ConfirmationTag,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(CONFIRMATION_TAG_LABEL, group_id)?;
        let value = encode_value(CONFIRMATION_TAG_LABEL, confirmation_tag)?;

        self.write::<CURRENT_VERSION>(CONFIRMATION_TAG_LABEL, &key, &value)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(public_key = %hex_kv(public_key)), err)]
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
        let value = encode_value(SIGNATURE_KEY_PAIR_LABEL, signature_key_pair)?;

        self.write::<CURRENT_VERSION>(SIGNATURE_KEY_PAIR_LABEL, &key, &value)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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
                let key = encode_key(QUEUED_PROPOSAL_LABEL, &(group_id, &proposal_ref))?;
                match self.read(QUEUED_PROPOSAL_LABEL, &key)? {
                    Some(proposal) => Ok((proposal_ref, proposal)),
                    None => Err(SqlKeyStoreError::NotFound),
                }
            })
            .collect::<Result<Vec<_>, _>>()
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(public_key = %hex_kv(public_key)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(hash_ref = %hex_kv(hash_ref)), err)]
    fn write_key_package<
        HashReference: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &HashReference,
        key_package: &KeyPackage,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &HashReference>(KEY_PACKAGE_LABEL, hash_ref)?;
        let value = encode_value(KEY_PACKAGE_LABEL, key_package)?;

        // Store the key package
        self.write::<CURRENT_VERSION>(KEY_PACKAGE_LABEL, &key, &value)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(_psk_id = %hex_kv(_psk_id)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(public_key = %hex_kv(public_key)), err)]
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
            &encode_value(ENCRYPTION_KEY_PAIR_LABEL, key_pair)?,
        )
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(hash_ref = %hex_kv(hash_ref)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(_psk_id = %hex_kv(_psk_id)), err)]
    fn psk<PskBundle: traits::PskBundle<CURRENT_VERSION>, PskId: traits::PskId<CURRENT_VERSION>>(
        &self,
        _psk_id: &PskId,
    ) -> Result<Option<PskBundle>, Self::Error> {
        Ok(None)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(public_key = %hex_kv(public_key)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(public_key = %hex_kv(public_key)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(public_key = %hex_kv(public_key)), err)]
    fn delete_encryption_key_pair<EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>>(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<(), Self::Error> {
        let key =
            build_key::<CURRENT_VERSION, &EncryptionKey>(ENCRYPTION_KEY_PAIR_LABEL, public_key)?;

        self.delete::<CURRENT_VERSION>(ENCRYPTION_KEY_PAIR_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(hash_ref = %hex_kv(hash_ref)), err)]
    fn delete_key_package<HashReference: traits::HashReference<CURRENT_VERSION>>(
        &self,
        hash_ref: &HashReference,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &HashReference>(KEY_PACKAGE_LABEL, hash_ref)?;
        self.delete::<CURRENT_VERSION>(KEY_PACKAGE_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(_psk_id = %hex_kv(_psk_id)), err)]
    fn delete_psk<PskKey: traits::PskId<CURRENT_VERSION>>(
        &self,
        _psk_id: &PskKey,
    ) -> Result<(), Self::Error> {
        Err(SqlKeyStoreError::UnsupportedMethod)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn write_group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_state: &GroupState,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(GROUP_STATE_LABEL, group_id)?;

        self.write::<CURRENT_VERSION>(
            GROUP_STATE_LABEL,
            &key,
            &encode_value(GROUP_STATE_LABEL, group_state)?,
        )
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn delete_group_state<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(GROUP_STATE_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(GROUP_STATE_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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
            &encode_value(MESSAGE_SECRETS_LABEL, message_secrets)?,
        )
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn delete_message_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(MESSAGE_SECRETS_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(MESSAGE_SECRETS_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ResumptionPskStore>, Self::Error> {
        self.read(
            RESUMPTION_PSK_STORE_LABEL,
            &encode_key(RESUMPTION_PSK_STORE_LABEL, group_id)?,
        )
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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
            &encode_key(RESUMPTION_PSK_STORE_LABEL, group_id)?,
            &encode_value(RESUMPTION_PSK_STORE_LABEL, resumption_psk_store)?,
        )
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn delete_all_resumption_psk_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.delete::<CURRENT_VERSION>(
            RESUMPTION_PSK_STORE_LABEL,
            &encode_key(RESUMPTION_PSK_STORE_LABEL, group_id)?,
        )
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id), own_leaf_index = %hex_kv(own_leaf_index)), err)]
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
            &encode_value(OWN_LEAF_NODE_INDEX_LABEL, own_leaf_index)?,
        )
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn delete_own_leaf_index<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(OWN_LEAF_NODE_INDEX_LABEL, group_id)?;
        self.delete::<CURRENT_VERSION>(OWN_LEAF_NODE_INDEX_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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
            &encode_value(EPOCH_SECRETS_LABEL, group_epoch_secrets)?,
        )
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn delete_group_epoch_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(EPOCH_SECRETS_LABEL, group_id)?;
        self.delete::<CURRENT_VERSION>(EPOCH_SECRETS_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id), epoch = %hex_kv(epoch), leaf_index = %leaf_index), err)]
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
        let value = encode_value(EPOCH_KEY_PAIRS_LABEL, key_pairs)?;
        tracing::trace!("Writing encryption epoch key pairs");

        self.write::<CURRENT_VERSION>(EPOCH_KEY_PAIRS_LABEL, &key, &value)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id), epoch = %hex_kv(epoch), leaf_index = %leaf_index), err)]
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

        let data: Vec<StorageData> = self.conn.raw_query(|conn| {
            sql_query(query)
                .bind::<diesel::sql_types::Binary, _>(&storage_key)
                .bind::<diesel::sql_types::Integer, _>(CURRENT_VERSION as i32)
                .load(conn)
        })?;

        if let Some(entry) = data.into_iter().next() {
            decode_value::<Vec<HpkeKeyPair>>(EPOCH_KEY_PAIRS_LABEL, &entry.value_bytes)
        } else {
            Ok(vec![])
        }
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id), epoch = %hex_kv(epoch), leaf_index = %leaf_index), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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
            let key = encode_key(QUEUED_PROPOSAL_LABEL, &(group_id, proposal_ref))?;
            self.delete::<CURRENT_VERSION>(QUEUED_PROPOSAL_LABEL, &key)?;
        }

        let key = build_key::<CURRENT_VERSION, &GroupId>(PROPOSAL_QUEUE_REFS_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(PROPOSAL_QUEUE_REFS_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
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

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id), config = %hex_kv(config)), err)]
    fn write_mls_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        config: &MlsGroupJoinConfig,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(JOIN_CONFIG_LABEL, group_id)?;
        let value = encode_value(JOIN_CONFIG_LABEL, config)?;

        self.write::<CURRENT_VERSION>(JOIN_CONFIG_LABEL, &key, &value)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn own_leaf_nodes<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<LeafNode>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(OWN_LEAF_NODES_LABEL, group_id)?;

        self.read_list(OWN_LEAF_NODES_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id), leaf_node = %hex_kv(leaf_node)), err)]
    fn append_own_leaf_node<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        leaf_node: &LeafNode,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(OWN_LEAF_NODES_LABEL, group_id)?;
        let value = encode_value(OWN_LEAF_NODES_LABEL, leaf_node)?;

        self.append::<CURRENT_VERSION>(OWN_LEAF_NODES_LABEL, &key, &value)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn delete_own_leaf_nodes<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(OWN_LEAF_NODES_LABEL, group_id)?;
        self.delete::<CURRENT_VERSION>(OWN_LEAF_NODES_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn delete_group_config<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(JOIN_CONFIG_LABEL, group_id)?;
        self.delete::<CURRENT_VERSION>(JOIN_CONFIG_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn delete_tree<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(TREE_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(TREE_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn delete_confirmation_tag<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(CONFIRMATION_TAG_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(CONFIRMATION_TAG_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn delete_context<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(GROUP_CONTEXT_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(GROUP_CONTEXT_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn delete_interim_transcript_hash<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(INTERIM_TRANSCRIPT_HASH_LABEL, group_id)?;

        self.delete::<CURRENT_VERSION>(INTERIM_TRANSCRIPT_HASH_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id), proposal_ref = %hex_kv(proposal_ref)), err)]
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
        let value = encode_value(PROPOSAL_QUEUE_REFS_LABEL, proposal_ref)?;
        self.remove_item::<CURRENT_VERSION>(PROPOSAL_QUEUE_REFS_LABEL, &key, &value)?;

        // Delete the proposal
        let key = encode_key(QUEUED_PROPOSAL_LABEL, &(group_id, proposal_ref))?;
        self.delete::<CURRENT_VERSION>(QUEUED_PROPOSAL_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn write_application_export_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ApplicationExportTree: traits::ApplicationExportTree<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        application_export_tree: &ApplicationExportTree,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(APPLICATION_EXPORT_TREE_LABEL, group_id)?;
        self.write::<CURRENT_VERSION>(
            APPLICATION_EXPORT_TREE_LABEL,
            &key,
            &encode_value(APPLICATION_EXPORT_TREE_LABEL, application_export_tree)?,
        )
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn application_export_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ApplicationExportTree: traits::ApplicationExportTree<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ApplicationExportTree>, Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(APPLICATION_EXPORT_TREE_LABEL, group_id)?;
        self.read(APPLICATION_EXPORT_TREE_LABEL, &key)
    }

    #[tracing::instrument(skip_all, target = OPENMLS_KV_TARGET, fields(group_id = %hex_kv(group_id)), err)]
    fn delete_application_export_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ApplicationExportTree: traits::ApplicationExportTree<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let key = build_key::<CURRENT_VERSION, &GroupId>(APPLICATION_EXPORT_TREE_LABEL, group_id)?;
        self.delete::<CURRENT_VERSION>(APPLICATION_EXPORT_TREE_LABEL, &key)
    }
}

/// Hex bincode of `v` for tracing fields. Empty string on serialize fail (field exprs can't propagate).
/// Record only storage keys and public values: never a value that holds a private key.
/// A `GroupContext` holds one: the commit-log signer in its AppData dictionary.
fn hex_kv<T: Serialize + ?Sized>(v: &T) -> String {
    bincode::serialize(v).map(hex::encode).unwrap_or_default()
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
    let key_vec = encode_key(label, &key)?;
    Ok(build_key_from_vec::<V>(label, key_vec))
}

/// One `(group id, epoch, leaf index)` tuple, not three concatenated
/// encodings: a self-describing codec does not frame its numbers, so two
/// different parts could encode to the same key. The key layout has no tuple
/// framing, so the bytes equal the old concatenation.
fn epoch_key_pairs_id(
    group_id: &impl traits::GroupId<CURRENT_VERSION>,
    epoch: &impl traits::EpochKey<CURRENT_VERSION>,
    leaf_index: u32,
) -> Result<Vec<u8>, SqlKeyStoreError> {
    encode_key(EPOCH_KEY_PAIRS_LABEL, &(group_id, epoch, leaf_index))
}

#[cfg(any(test, feature = "test-utils"))]
impl SqlKeyStore<crate::test_utils::MemoryStorage> {
    pub fn kv_pairs(&self) -> String {
        self.conn.key_value_pairs()
    }

    pub fn kv_pairs_utf8(&self) -> String {
        self.conn.key_value_pairs_utf8()
    }
}

#[cfg(test)]
pub(crate) mod tests {
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
    use crate::TransactionOutcome::{Continue, Rollback};
    use crate::encrypted_store::MlsProviderExt;
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

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_read_write() {
        let store = crate::TestDb::create_persistent_store(None).await;
        let conn = store.conn();
        let mls_store = SqlKeyStore::new(conn);
        let provider = XmtpOpenMlsProvider::new(mls_store);
        let key_store = provider.key_store();

        use crate::sql_key_store::{
            COMMIT_LOG_SIGNER_PRIVATE_KEY as LABEL,
            codec::{encode_key, encode_value},
        };
        let raw_value = vec![3u8; 32];
        let group_1 = encode_key(LABEL, &[1u8; 32])?;
        let group_2 = encode_key(LABEL, &[2u8; 32])?;
        let value_1 = encode_value(LABEL, &raw_value)?;

        key_store.write::<CURRENT_VERSION>(
            crate::sql_key_store::COMMIT_LOG_SIGNER_PRIVATE_KEY,
            &group_1,
            &value_1,
        )?;

        // Query on a value that hasn't been written
        let result = key_store.read::<CURRENT_VERSION, Vec<u8>>(
            crate::sql_key_store::COMMIT_LOG_SIGNER_PRIVATE_KEY,
            &group_2,
        );
        assert!(result.is_ok(), "{}", result.err().unwrap());
        assert!(result.unwrap().is_none());

        let result = key_store.read::<CURRENT_VERSION, Vec<u8>>(
            crate::sql_key_store::COMMIT_LOG_SIGNER_PRIVATE_KEY,
            &group_1,
        );
        assert!(result.is_ok(), "{}", result.err().unwrap());
        assert_eq!(result.unwrap(), Some(raw_value));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn transaction_commit_persists_rollback_does_not_and_error_propagates() {
        use crate::{
            StorageError, TransactionOutcome, traits::TransactionalKeyStore,
            xmtp_openmls_provider::XmtpMlsStorageProvider,
        };

        let store = crate::TestDb::create_persistent_store(None).await;
        let conn = store.conn();
        let key_store = SqlKeyStore::new(conn);

        use crate::sql_key_store::{
            COMMIT_LOG_SIGNER_PRIVATE_KEY as LABEL,
            codec::{encode_key, encode_value},
        };
        let committed_key = encode_key(LABEL, &[10u8; 32])?;
        let rolled_back_key = encode_key(LABEL, &[11u8; 32])?;
        let value = encode_value(LABEL, &vec![7u8; 16])?;

        let is_present = |k: &[u8]| {
            key_store
                .read::<CURRENT_VERSION, Vec<u8>>(
                    crate::sql_key_store::COMMIT_LOG_SIGNER_PRIVATE_KEY,
                    k,
                )
                .unwrap()
                .is_some()
        };

        // Commit: a value written inside a committing transaction is visible after.
        let outcome = key_store
            .transaction(|conn| {
                conn.key_store().write::<CURRENT_VERSION>(
                    crate::sql_key_store::COMMIT_LOG_SIGNER_PRIVATE_KEY,
                    &committed_key,
                    &value,
                )?;
                Ok::<_, StorageError>(Continue(()))
            })
            .unwrap();
        assert!(matches!(outcome, Continue(())));
        assert!(is_present(&committed_key), "commit must persist");

        // Rollback: returns Ok(Rollback), NOT an Err, and the write is discarded.
        let outcome = key_store
            .transaction(|conn| {
                conn.key_store().write::<CURRENT_VERSION>(
                    crate::sql_key_store::COMMIT_LOG_SIGNER_PRIVATE_KEY,
                    &rolled_back_key,
                    &value,
                )?;
                Ok::<TransactionOutcome<()>, StorageError>(Rollback)
            })
            .unwrap();
        assert!(matches!(outcome, Rollback));
        assert!(!is_present(&rolled_back_key), "rollback must not persist");

        // Real error: a closure returning Err propagates as Err (and rolls back).
        let result: Result<TransactionOutcome<()>, StorageError> =
            key_store.transaction(|_conn| Err(StorageError::DbSerialize));
        assert!(matches!(result, Err(StorageError::DbSerialize)));
    }

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

    #[xmtp_common::test]
    async fn application_export_tree() {
        let store = crate::TestDb::create_persistent_store(None).await;
        let conn = store.conn();
        let store = SqlKeyStore::new(conn);
        let provider = XmtpOpenMlsProvider::new(store);

        #[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
        struct AppExportTree {
            nodes: Vec<Vec<u8>>,
            leaf_count: u32,
            root_hash: [u8; 32],
        }
        impl traits::ApplicationExportTree<CURRENT_VERSION> for AppExportTree {}
        impl Entity<CURRENT_VERSION> for AppExportTree {}

        fn random_tree(rand: &impl openmls_traits::random::OpenMlsRand) -> AppExportTree {
            let leaf_count = 3 + (rand.random_vec(1).unwrap()[0] % 10) as u32;
            let nodes: Vec<Vec<u8>> = (0..leaf_count)
                .map(|_| rand.random_vec(64).unwrap())
                .collect();
            AppExportTree {
                nodes,
                leaf_count,
                root_hash: xmtp_common::rand_array(),
            }
        }

        let group_id_1 = GroupId::random(provider.rand());
        let group_id_2 = GroupId::random(provider.rand());
        let tree_1 = random_tree(provider.rand());
        let tree_2 = random_tree(provider.rand());

        // Read before write should return None
        let result: Option<AppExportTree> = provider
            .storage()
            .application_export_tree(&group_id_1)
            .unwrap();
        assert!(result.is_none());

        // Write tree for group 1
        provider
            .storage()
            .write_application_export_tree(&group_id_1, &tree_1)
            .unwrap();

        // Read back group 1
        let result: Option<AppExportTree> = provider
            .storage()
            .application_export_tree(&group_id_1)
            .unwrap();
        assert_eq!(result.unwrap(), tree_1);

        // Group 2 should still be None
        let result: Option<AppExportTree> = provider
            .storage()
            .application_export_tree(&group_id_2)
            .unwrap();
        assert!(result.is_none());

        // Write tree for group 2
        provider
            .storage()
            .write_application_export_tree(&group_id_2, &tree_2)
            .unwrap();
        let result: Option<AppExportTree> = provider
            .storage()
            .application_export_tree(&group_id_2)
            .unwrap();
        assert_eq!(result.unwrap(), tree_2);

        // Overwrite group 1 with new data
        let tree_1_updated = random_tree(provider.rand());
        provider
            .storage()
            .write_application_export_tree(&group_id_1, &tree_1_updated)
            .unwrap();
        let result: Option<AppExportTree> = provider
            .storage()
            .application_export_tree(&group_id_1)
            .unwrap();
        assert_eq!(result.unwrap(), tree_1_updated);

        // Group 2 should be unaffected
        let result: Option<AppExportTree> = provider
            .storage()
            .application_export_tree(&group_id_2)
            .unwrap();
        assert_eq!(result.unwrap(), tree_2);

        // Delete group 1
        provider
            .storage()
            .delete_application_export_tree::<GroupId, AppExportTree>(&group_id_1)
            .unwrap();
        let result: Option<AppExportTree> = provider
            .storage()
            .application_export_tree(&group_id_1)
            .unwrap();
        assert!(result.is_none());

        // Group 2 should still exist
        let result: Option<AppExportTree> = provider
            .storage()
            .application_export_tree(&group_id_2)
            .unwrap();
        assert_eq!(result.unwrap(), tree_2);

        // Delete group 2
        provider
            .storage()
            .delete_application_export_tree::<GroupId, AppExportTree>(&group_id_2)
            .unwrap();
        let result: Option<AppExportTree> = provider
            .storage()
            .application_export_tree(&group_id_2)
            .unwrap();
        assert!(result.is_none());

        // Deleting again should not error
        provider
            .storage()
            .delete_application_export_tree::<GroupId, AppExportTree>(&group_id_1)
            .unwrap();
    }

    /// An HPKE key pair as a key store entity. OpenMLS keeps its own
    /// `EncryptionKeyPair` crate-private.
    #[derive(Serialize, Deserialize, Clone)]
    struct KeyPair(openmls_traits::types::HpkeKeyPair);
    impl traits::HpkeKeyPair<CURRENT_VERSION> for KeyPair {}
    impl Entity<CURRENT_VERSION> for KeyPair {}

    /// The value bytes stored under `label` and `key`, as `write` stores them.
    fn stored_value<C: crate::ConnectionExt>(
        store: &SqlKeyStore<C>,
        label: &[u8],
        key: &[u8],
    ) -> Vec<u8> {
        let storage_key = super::build_key_from_vec::<CURRENT_VERSION>(label, key.to_vec());
        let rows = store.select_query::<CURRENT_VERSION>(&storage_key).unwrap();
        rows.into_iter()
            .next()
            .expect("a stored value")
            .value_bytes
            .to_vec()
    }

    /// Wiping changes no stored bytes. Each key package entity, the
    /// post-quantum wrapper key and each list rewrite store the bincode that
    /// the code before wiping stored for the same value.
    #[xmtp_common::test(unwrap_try = true)]
    async fn stored_bytes_are_unchanged() {
        use super::{
            ENCRYPTION_KEY_PAIR_LABEL, EPOCH_KEY_PAIRS_LABEL, KEY_PACKAGE_LABEL,
            KEY_PACKAGE_WRAPPER_PRIVATE_KEY, SIGNATURE_KEY_PAIR_LABEL, build_key,
            epoch_key_pairs_id,
        };
        use crate::xmtp_openmls_provider::XmtpMlsStorageProvider;
        use openmls::{prelude::GroupEpoch, treesync::EncryptionKey};
        use openmls_traits::crypto::OpenMlsCrypto;

        let store = crate::TestDb::create_persistent_store(None).await;
        let provider = XmtpOpenMlsProvider::new(SqlKeyStore::new(store.conn()));
        let key_store = provider.storage();

        let bundle = crate::test_utils::generate_key_package_bundle();
        let hash_ref = bundle.key_package().hash_ref(provider.crypto())?;
        key_store.write_key_package(&hash_ref, &bundle)?;
        let key = build_key::<CURRENT_VERSION, _>(KEY_PACKAGE_LABEL, &hash_ref)?;
        assert!(
            stored_value(key_store, KEY_PACKAGE_LABEL, &key) == bincode::serialize(&bundle)?,
            "key package bundle"
        );

        let key_pair = KeyPair(
            provider
                .crypto()
                .derive_hpke_keypair(CIPHERSUITE.hpke_config(), &[1; 32])?,
        );
        let public_key = EncryptionKey::from(key_pair.0.public.clone());
        key_store.write_encryption_key_pair(&public_key, &key_pair)?;
        let key = build_key::<CURRENT_VERSION, _>(ENCRYPTION_KEY_PAIR_LABEL, &public_key)?;
        assert!(
            stored_value(key_store, ENCRYPTION_KEY_PAIR_LABEL, &key)
                == bincode::serialize(&key_pair)?,
            "encryption key pair"
        );

        let signature_key_pair = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        let public_key = StorageId::from(signature_key_pair.to_public_vec());
        key_store.write_signature_key_pair(&public_key, &signature_key_pair)?;
        let key = build_key::<CURRENT_VERSION, _>(SIGNATURE_KEY_PAIR_LABEL, &public_key)?;
        assert!(
            stored_value(key_store, SIGNATURE_KEY_PAIR_LABEL, &key)
                == bincode::serialize(&signature_key_pair)?,
            "signature key pair"
        );

        let group_id = GroupId::from_slice(b"stored bytes");
        let epoch = GroupEpoch::from(3);
        let epoch_key_pairs = [
            key_pair.clone(),
            KeyPair(
                provider
                    .crypto()
                    .derive_hpke_keypair(CIPHERSUITE.hpke_config(), &[2; 32])?,
            ),
        ];
        key_store.write_encryption_epoch_key_pairs(&group_id, &epoch, 1, &epoch_key_pairs)?;
        let key = epoch_key_pairs_id(&group_id, &epoch, 1)?;
        assert!(
            stored_value(key_store, EPOCH_KEY_PAIRS_LABEL, &key)
                == bincode::serialize(epoch_key_pairs.as_slice())?,
            "epoch key pairs"
        );

        // The post-quantum key compares with the expression `identity.rs` used
        // before it called `write_wrapper_private_key`.
        let post_quantum = xmtp_cryptography::post_quantum::generate_post_quantum_key()?;
        let hash_ref = bincode::serialize(&hash_ref)?;
        key_store.write_wrapper_private_key(&hash_ref, &post_quantum.private)?;
        assert!(
            stored_value(key_store, KEY_PACKAGE_WRAPPER_PRIVATE_KEY, &hash_ref)
                == bincode::serialize(&post_quantum.private.to_vec())?,
            "post-quantum wrapper private key"
        );

        const LIST: &[u8] = b"StoredBytesList";
        let (first, second) = (b"first entry".as_slice(), b"second entry".as_slice());
        key_store.append::<CURRENT_VERSION>(LIST, b"key", first)?;
        assert!(
            stored_value(key_store, LIST, b"key") == bincode::serialize(&vec![first])?,
            "first list entry"
        );
        key_store.append::<CURRENT_VERSION>(LIST, b"key", second)?;
        assert!(
            stored_value(key_store, LIST, b"key") == bincode::serialize(&vec![first, second])?,
            "appended list entry"
        );
        key_store.remove_item::<CURRENT_VERSION>(LIST, b"key", first)?;
        assert!(
            stored_value(key_store, LIST, b"key") == bincode::serialize(&vec![second])?,
            "removed list entry"
        );
    }

    /// A value for every OpenMLS value trait. Its long byte string makes the
    /// bincode length exceed 65,535.
    #[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
    struct Value(Vec<u8>, u64);
    impl Entity<CURRENT_VERSION> for Value {}
    macro_rules! value_traits {
        ($($name:ident),*) => { $(impl traits::$name<CURRENT_VERSION> for Value {})* };
    }
    value_traits!(
        QueuedProposal,
        TreeSync,
        GroupContext,
        InterimTranscriptHash,
        ConfirmationTag,
        SignatureKeyPair,
        HpkeKeyPair,
        GroupState,
        GroupEpochSecrets,
        LeafNodeIndex,
        MessageSecrets,
        ResumptionPskStore,
        KeyPackage,
        MlsGroupJoinConfig,
        LeafNode,
        ApplicationExportTree
    );

    fn value(n: u8) -> Value {
        Value(vec![n; 70_000], u64::from(n))
    }

    /// Every label stores the bytes the code before the codec module stored.
    /// The expected storage keys and values are built by hand from
    /// `bincode::serialize`. Every value reads back, and every delete removes
    /// the row at that storage key.
    #[xmtp_common::test(unwrap_try = true)]
    async fn every_label_keeps_todays_bytes() {
        use super::*;
        use openmls::{
            prelude::{
                GroupEpoch, KeyPackageRef,
                tls_codec::{Deserialize as _, Serialize as _, VLBytes},
            },
            treesync::EncryptionKey,
        };

        let store = crate::TestDb::create_persistent_store(None).await;
        let key_store = SqlKeyStore::new(store.conn());
        const VERSION: [u8; 2] = CURRENT_VERSION.to_be_bytes();
        // `build_key`, then `write`: `label || label || key || version || version`.
        let doubled = |label: &[u8], key: &[u8]| [label, label, key, &VERSION, &VERSION].concat();
        // `write` with a key built in place: `label || key || version`.
        let single = |label: &[u8], key: &[u8]| [label, key, &VERSION].concat();
        let stored = |storage_key: Vec<u8>| {
            key_store
                .select_query::<CURRENT_VERSION>(&storage_key)
                .unwrap()
                .into_iter()
                .next()
                .map(|row| row.value_bytes.to_vec())
        };
        let hash_ref = |byte: u8| {
            let tls = VLBytes::new(vec![byte; 32])
                .tls_serialize_detached()
                .unwrap();
            KeyPackageRef::tls_deserialize_exact(tls).unwrap()
        };

        let group_id = GroupId::from_slice(&[0x5a; 300]);
        let group_key = bincode::serialize(&group_id)?;

        macro_rules! group_value {
            ($label:expr, $write:ident, $read:ident, $delete:expr, $n:expr) => {{
                let v = value($n);
                let storage_key = doubled($label, &group_key);
                key_store.$write(&group_id, &v)?;
                assert_eq!(
                    stored(storage_key.clone()),
                    Some(bincode::serialize(&v)?),
                    "{}",
                    String::from_utf8_lossy($label)
                );
                let read: Option<Value> = key_store.$read(&group_id)?;
                assert_eq!(read, Some(v));
                $delete;
                assert_eq!(
                    stored(storage_key),
                    None,
                    "{} delete",
                    String::from_utf8_lossy($label)
                );
            }};
        }
        group_value!(
            TREE_LABEL,
            write_tree,
            tree,
            key_store.delete_tree(&group_id)?,
            1
        );
        group_value!(
            INTERIM_TRANSCRIPT_HASH_LABEL,
            write_interim_transcript_hash,
            interim_transcript_hash,
            key_store.delete_interim_transcript_hash(&group_id)?,
            2
        );
        group_value!(
            GROUP_CONTEXT_LABEL,
            write_context,
            group_context,
            key_store.delete_context(&group_id)?,
            3
        );
        group_value!(
            CONFIRMATION_TAG_LABEL,
            write_confirmation_tag,
            confirmation_tag,
            key_store.delete_confirmation_tag(&group_id)?,
            4
        );
        group_value!(
            GROUP_STATE_LABEL,
            write_group_state,
            group_state,
            key_store.delete_group_state(&group_id)?,
            5
        );
        group_value!(
            MESSAGE_SECRETS_LABEL,
            write_message_secrets,
            message_secrets,
            key_store.delete_message_secrets(&group_id)?,
            6
        );
        group_value!(
            OWN_LEAF_NODE_INDEX_LABEL,
            write_own_leaf_index,
            own_leaf_index,
            key_store.delete_own_leaf_index(&group_id)?,
            7
        );
        group_value!(
            EPOCH_SECRETS_LABEL,
            write_group_epoch_secrets,
            group_epoch_secrets,
            key_store.delete_group_epoch_secrets(&group_id)?,
            8
        );
        group_value!(
            JOIN_CONFIG_LABEL,
            write_mls_join_config,
            mls_group_join_config,
            key_store.delete_group_config(&group_id)?,
            9
        );
        group_value!(
            APPLICATION_EXPORT_TREE_LABEL,
            write_application_export_tree,
            application_export_tree,
            key_store.delete_application_export_tree::<_, Value>(&group_id)?,
            10
        );

        // The resumption PSK store key holds the label once.
        let v = value(11);
        key_store.write_resumption_psk_store(&group_id, &v)?;
        assert_eq!(
            stored(single(RESUMPTION_PSK_STORE_LABEL, &group_key)),
            Some(bincode::serialize(&v)?)
        );
        let read: Option<Value> = key_store.resumption_psk_store(&group_id)?;
        assert_eq!(read, Some(v));
        key_store.delete_all_resumption_psk_secrets(&group_id)?;
        assert_eq!(stored(single(RESUMPTION_PSK_STORE_LABEL, &group_key)), None);

        let public_key = StorageId::from(vec![0x12; 32]);
        let v = value(12);
        key_store.write_signature_key_pair(&public_key, &v)?;
        assert_eq!(
            stored(doubled(
                SIGNATURE_KEY_PAIR_LABEL,
                &bincode::serialize(&public_key)?
            )),
            Some(bincode::serialize(&v)?)
        );
        let read: Option<Value> = key_store.signature_key_pair(&public_key)?;
        assert_eq!(read, Some(v));
        key_store.delete_signature_key_pair(&public_key)?;
        assert_eq!(
            stored(doubled(
                SIGNATURE_KEY_PAIR_LABEL,
                &bincode::serialize(&public_key)?
            )),
            None
        );

        let key_package_ref = hash_ref(0x13);
        let v = value(13);
        key_store.write_key_package(&key_package_ref, &v)?;
        assert_eq!(
            stored(doubled(
                KEY_PACKAGE_LABEL,
                &bincode::serialize(&key_package_ref)?
            )),
            Some(bincode::serialize(&v)?)
        );
        let read: Option<Value> = key_store.key_package(&key_package_ref)?;
        assert_eq!(read, Some(v));
        key_store.delete_key_package(&key_package_ref)?;
        assert_eq!(
            stored(doubled(
                KEY_PACKAGE_LABEL,
                &bincode::serialize(&key_package_ref)?
            )),
            None
        );

        let encryption_key = EncryptionKey::from(vec![0x14; 32]);
        let v = value(14);
        key_store.write_encryption_key_pair(&encryption_key, &v)?;
        assert_eq!(
            stored(doubled(
                ENCRYPTION_KEY_PAIR_LABEL,
                &bincode::serialize(&encryption_key)?
            )),
            Some(bincode::serialize(&v)?)
        );
        let read: Option<Value> = key_store.encryption_key_pair(&encryption_key)?;
        assert_eq!(read, Some(v));
        key_store.delete_encryption_key_pair(&encryption_key)?;
        assert_eq!(
            stored(doubled(
                ENCRYPTION_KEY_PAIR_LABEL,
                &bincode::serialize(&encryption_key)?
            )),
            None
        );

        // The epoch key pairs key is the three parts back to back.
        let epoch = GroupEpoch::from(15);
        let pairs = [value(15), value(16)];
        key_store.write_encryption_epoch_key_pairs(&group_id, &epoch, 17, &pairs)?;
        let parts = [
            group_key.clone(),
            bincode::serialize(&epoch)?,
            bincode::serialize(&17u32)?,
        ]
        .concat();
        assert_eq!(
            stored(single(EPOCH_KEY_PAIRS_LABEL, &parts)),
            Some(bincode::serialize(pairs.as_slice())?)
        );
        let read: Vec<Value> = key_store.encryption_epoch_key_pairs(&group_id, &epoch, 17)?;
        assert_eq!(read, pairs);
        key_store.delete_encryption_epoch_key_pairs(&group_id, &epoch, 17)?;
        assert_eq!(stored(single(EPOCH_KEY_PAIRS_LABEL, &parts)), None);

        // Lists: each item is the bincode of one value.
        let (first, second) = (value(18), value(19));
        key_store.append_own_leaf_node(&group_id, &first)?;
        key_store.append_own_leaf_node(&group_id, &second)?;
        assert_eq!(
            stored(doubled(OWN_LEAF_NODES_LABEL, &group_key)),
            Some(bincode::serialize(&vec![
                bincode::serialize(&first)?,
                bincode::serialize(&second)?
            ])?)
        );
        let read: Vec<Value> = key_store.own_leaf_nodes(&group_id)?;
        assert_eq!(read, [first, second]);
        key_store.delete_own_leaf_nodes(&group_id)?;
        assert_eq!(stored(doubled(OWN_LEAF_NODES_LABEL, &group_key)), None);

        let (ref_1, ref_2) = (hash_ref(0x20), hash_ref(0x21));
        let (proposal_1, proposal_2) = (value(20), value(21));
        key_store.queue_proposal(&group_id, &ref_1, &proposal_1)?;
        key_store.queue_proposal(&group_id, &ref_2, &proposal_2)?;
        assert_eq!(
            stored(single(
                QUEUED_PROPOSAL_LABEL,
                &bincode::serialize(&(&group_id, &ref_1))?
            )),
            Some(bincode::serialize(&proposal_1)?)
        );
        let refs_key = doubled(PROPOSAL_QUEUE_REFS_LABEL, &group_key);
        assert_eq!(
            stored(refs_key.clone()),
            Some(bincode::serialize(&vec![
                bincode::serialize(&ref_1)?,
                bincode::serialize(&ref_2)?
            ])?)
        );
        let read: Vec<(KeyPackageRef, Value)> = key_store.queued_proposals(&group_id)?;
        assert_eq!(
            read,
            [(ref_1.clone(), proposal_1), (ref_2.clone(), proposal_2)]
        );
        let proposal_key =
            |proposal_ref: &KeyPackageRef| -> Result<Vec<u8>, Box<dyn std::error::Error>> {
                Ok(single(
                    QUEUED_PROPOSAL_LABEL,
                    &bincode::serialize(&(&group_id, proposal_ref))?,
                ))
            };
        key_store.remove_proposal(&group_id, &ref_1)?;
        assert_eq!(
            stored(refs_key.clone()),
            Some(bincode::serialize(&vec![bincode::serialize(&ref_2)?])?)
        );
        assert_eq!(stored(proposal_key(&ref_1)?), None);
        assert!(stored(proposal_key(&ref_2)?).is_some());
        key_store.clear_proposal_queue::<_, KeyPackageRef>(&group_id)?;
        assert_eq!(stored(proposal_key(&ref_2)?), None);
        assert_eq!(stored(refs_key), None);

        // A removal from a missing list writes a one-item array: no list
        // length, so the row does not decode as a list. Kept as it is today.
        let other_group = GroupId::from_slice(b"no queue");
        key_store.remove_proposal(&other_group, &ref_2)?;
        let ref_bytes = bincode::serialize(&ref_2)?;
        assert_eq!(
            stored(doubled(
                PROPOSAL_QUEUE_REFS_LABEL,
                &bincode::serialize(&other_group)?
            )),
            Some(bincode::serialize(&[ref_bytes.as_slice()])?)
        );
    }

    /// The `write_key_package` span records the hash reference, never the
    /// bundle with its init and leaf encryption private keys.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn write_key_package_span_records_no_private_key() {
        let store = crate::TestDb::create_persistent_store(None).await;
        let provider = XmtpOpenMlsProvider::new(SqlKeyStore::new(store.conn()));
        let bundle = crate::test_utils::generate_key_package_bundle();
        let hash_ref = bundle.key_package().hash_ref(provider.crypto())?;

        let (written, spans) = xmtp_logging::test_logging::with_trace_layer(true, || {
            provider.storage().write_key_package(&hash_ref, &bundle)
        });
        written?;
        let span = spans
            .iter()
            .find(|span| span.name == "write_key_package")
            .expect("the key store exports its write_key_package span");
        let attributes = format!("{:?}", span.attributes);
        assert!(
            attributes.contains(&hex::encode(bincode::serialize(&hash_ref)?)),
            "the span records the hash reference"
        );
        for private_key in [bundle.init_private_key(), bundle.encryption_private_key()] {
            assert!(
                !attributes.contains(&hex::encode(&**private_key)),
                "the span records a private key"
            );
        }
    }

    /// The group context's AppData dictionary holds the commit-log signer, a
    /// group-shared private key, and OpenMLS writes the context on every epoch
    /// change. The `write_context` span records the group id, never the
    /// context, and no other key store span records the signer.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn write_context_span_records_no_private_key() {
        use openmls::{
            extensions::{
                AppDataDictionary, AppDataDictionaryExtension, Extension, ExtensionType, Extensions,
            },
            group::{GroupContext, MlsGroup},
            prelude::{BasicCredential, Capabilities, CredentialWithKey},
        };

        // `ComponentId::COMMIT_LOG_SIGNER` in `xmtp_mls_common`. Its data is
        // the raw 32-byte signer key.
        const COMMIT_LOG_SIGNER: u16 = 0x800B;

        let store = crate::TestDb::create_persistent_store(None).await;
        let provider = XmtpOpenMlsProvider::new(SqlKeyStore::new(store.conn()));
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        let credential_with_key = CredentialWithKey {
            credential: BasicCredential::new(b"group context test".to_vec()).into(),
            signature_key: signer.to_public_vec().into(),
        };
        let commit_log_signer = xmtp_cryptography::rand::rand_vec::<32>();
        let mut dictionary = AppDataDictionary::new();
        let _ = dictionary.insert(COMMIT_LOG_SIGNER, commit_log_signer.clone());
        let extensions = Extensions::single(Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dictionary),
        ))?;
        let capabilities = Capabilities::new(
            None,
            None,
            Some(&[ExtensionType::AppDataDictionary]),
            None,
            None,
        );

        // OpenMLS writes the context itself while it creates the group.
        let (group, spans) = xmtp_logging::test_logging::with_trace_layer(true, || {
            MlsGroup::builder()
                .ciphersuite(CIPHERSUITE)
                .with_capabilities(capabilities)
                .with_group_context_extensions(extensions)
                .build(&provider, &signer, credential_with_key)
        });
        let group = group?;
        let signer_hex = hex::encode(&commit_log_signer);
        let stored_context: GroupContext = provider
            .storage()
            .group_context(group.group_id())?
            .expect("OpenMLS stores the group context");
        assert!(
            hex::encode(bincode::serialize(&stored_context)?).contains(&signer_hex),
            "the stored group context holds the commit-log signer"
        );

        let span = spans
            .iter()
            .find(|span| span.name == "write_context")
            .expect("the key store exports its write_context span");
        assert!(
            format!("{:?}", span.attributes)
                .contains(&hex::encode(bincode::serialize(group.group_id())?)),
            "the span records the group id"
        );
        for span in &spans {
            assert!(
                !format!("{:?}", span.attributes).contains(&signer_hex),
                "the {} span records the commit-log signer",
                span.name
            );
        }
    }
}
