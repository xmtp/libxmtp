//! Async-track OpenMLS key store: [`PgKeyStore`].
//!
//! The async (sqlx/Postgres) counterpart of the sync-track
//! [`SqlKeyStore`](crate::sql_key_store::SqlKeyStore). It implements the OpenMLS
//! [`StorageProvider`] and libxmtp's [`XmtpMlsStorageProvider`] over a [`PgDb`]
//! handle, delegating every one of the ~56 storage methods to
//! [`PostgresStorageProvider`] -- the real Postgres implementation that lives in
//! the `openmls_pg_storage` crate. Each delegating body borrows one connection
//! from the handle and runs the corresponding `async` method on a
//! freshly-wrapped provider; inside a transaction that connection is the pinned
//! one, so the key-store writes land atomically with libxmtp's own table writes.
//!
//! Servers only; never wasm. Gated behind the `async` feature.

use serde::Serialize;
use serde::de::DeserializeOwned;

use openmls_pg_storage::{Codec, PostgresStorageProvider};
use openmls_traits::storage::{CURRENT_VERSION, Entity, StorageProvider, traits};

use crate::pg::PgDb;
use crate::xmtp_openmls_provider::{TransactionOutcome, TxFn};
use crate::{SqlKeyStoreError, TransactionalKeyStore, XmtpMlsStorageProvider};

/// The CBOR codec used to (de)serialize every stored OpenMLS value on the
/// async/Postgres track.
///
/// CBOR is self-describing, so it round-trips OpenMLS entities whose serde
/// shapes bincode cannot (bincode is not self-describing and rejects e.g.
/// `deserialize_any`/untagged forms). The sync/SQLite track still serializes
/// with bincode; moving it to CBOR needs a data migration and is deferred.
#[derive(Default)]
pub struct CborCodec;

/// ciborium's serialize and deserialize errors are distinct types; unify them
/// into one concrete error so it satisfies the `Codec::Error` bounds
/// (Error + Debug + Send + Sync + 'static).
#[derive(Debug, thiserror::Error)]
#[error("cbor codec: {0}")]
pub struct CborCodecError(String);

impl Codec for CborCodec {
    type Error = CborCodecError;

    fn to_vec<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, Self::Error> {
        let mut buf = Vec::new();
        ciborium::into_writer(value, &mut buf).map_err(|e| CborCodecError(e.to_string()))?;
        Ok(buf)
    }

    fn from_slice<T: DeserializeOwned>(slice: &[u8]) -> Result<T, Self::Error> {
        ciborium::from_reader(slice)
            .map_err(|e| CborCodecError(format!("{e} (decoding {})", std::any::type_name::<T>())))
    }
}

/// Async-track MLS key store: an OpenMLS [`StorageProvider`] backed by a
/// [`PgDb`] handle. Cheap to clone; clones share one backend.
#[derive(Clone)]
pub struct PgKeyStore {
    db: PgDb,
}

impl PgKeyStore {
    pub fn new(db: PgDb) -> Self {
        Self { db }
    }

    /// Shared SELECT for the generic KV `read`/`read_list`: the raw
    /// `value_bytes` stored under `label || key || version_be`, if present.
    async fn kv_select(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<Option<Vec<u8>>, SqlKeyStoreError> {
        let storage_key = build_kv_key(label, key);
        let mut c = self.db.conn().await?;
        let row: Option<(Vec<u8>,)> = sqlx::query_as(
            "SELECT value_bytes FROM openmls_key_value WHERE key_bytes = $1 AND version = $2",
        )
        .bind(storage_key)
        .bind(CURRENT_VERSION as i32)
        .fetch_optional(&mut *c)
        .await
        .map_err(crate::ConnectionError::from)?;
        Ok(row.map(|(v,)| v))
    }
}

/// The generic-KV storage key: `label || key || version_be`. Byte-identical to
/// the sync track's `build_key_from_vec::<CURRENT_VERSION>`, so a database ever
/// shared between the two backends would agree on the layout.
fn build_kv_key(label: &[u8], key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(label.len() + key.len() + 2);
    out.extend_from_slice(label);
    out.extend_from_slice(key);
    out.extend_from_slice(&u16::to_be_bytes(CURRENT_VERSION));
    out
}


impl StorageProvider<CURRENT_VERSION> for PgKeyStore {
    type Error = SqlKeyStoreError;

    async fn write_mls_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        config: &MlsGroupJoinConfig,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_mls_join_config::<GroupId, MlsGroupJoinConfig>(group_id, config)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn append_own_leaf_node<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        leaf_node: &LeafNode,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .append_own_leaf_node::<GroupId, LeafNode>(group_id, leaf_node)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn queue_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
        proposal: &QueuedProposal,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .queue_proposal::<GroupId, ProposalRef, QueuedProposal>(group_id, proposal_ref, proposal)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        tree: &TreeSync,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_tree::<GroupId, TreeSync>(group_id, tree)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        interim_transcript_hash: &InterimTranscriptHash,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_interim_transcript_hash::<GroupId, InterimTranscriptHash>(group_id, interim_transcript_hash)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_context: &GroupContext,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_context::<GroupId, GroupContext>(group_id, group_context)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        confirmation_tag: &ConfirmationTag,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_confirmation_tag::<GroupId, ConfirmationTag>(group_id, confirmation_tag)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_state: &GroupState,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_group_state::<GroupState, GroupId>(group_id, group_state)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        message_secrets: &MessageSecrets,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_message_secrets::<GroupId, MessageSecrets>(group_id, message_secrets)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        resumption_psk_store: &ResumptionPskStore,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_resumption_psk_store::<GroupId, ResumptionPskStore>(group_id, resumption_psk_store)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        own_leaf_index: &LeafNodeIndex,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_own_leaf_index::<GroupId, LeafNodeIndex>(group_id, own_leaf_index)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_epoch_secrets: &GroupEpochSecrets,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_group_epoch_secrets::<GroupId, GroupEpochSecrets>(group_id, group_epoch_secrets)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
        signature_key_pair: &SignatureKeyPair,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_signature_key_pair::<SignaturePublicKey, SignatureKeyPair>(public_key, signature_key_pair)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_encryption_key_pair<
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
        key_pair: &HpkeKeyPair,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_encryption_key_pair::<EncryptionKey, HpkeKeyPair>(public_key, key_pair)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_encryption_epoch_key_pairs<
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
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_encryption_epoch_key_pairs::<GroupId, EpochKey, HpkeKeyPair>(group_id, epoch, leaf_index, key_pairs)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_key_package<
        HashReference: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &HashReference,
        key_package: &KeyPackage,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_key_package::<HashReference, KeyPackage>(hash_ref, key_package)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_psk<
        PskId: traits::PskId<CURRENT_VERSION>,
        PskBundle: traits::PskBundle<CURRENT_VERSION>,
    >(
        &self,
        psk_id: &PskId,
        psk: &PskBundle,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_psk::<PskId, PskBundle>(psk_id, psk)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn mls_group_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MlsGroupJoinConfig>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .mls_group_join_config::<GroupId, MlsGroupJoinConfig>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn own_leaf_nodes<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<LeafNode>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .own_leaf_nodes::<GroupId, LeafNode>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn queued_proposal_refs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<ProposalRef>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .queued_proposal_refs::<GroupId, ProposalRef>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn queued_proposals<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<(ProposalRef, QueuedProposal)>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .queued_proposals::<GroupId, ProposalRef, QueuedProposal>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<TreeSync>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .tree::<GroupId, TreeSync>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn group_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupContext>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .group_context::<GroupId, GroupContext>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<InterimTranscriptHash>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .interim_transcript_hash::<GroupId, InterimTranscriptHash>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ConfirmationTag>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .confirmation_tag::<GroupId, ConfirmationTag>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupState>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .group_state::<GroupState, GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MessageSecrets>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .message_secrets::<GroupId, MessageSecrets>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ResumptionPskStore>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .resumption_psk_store::<GroupId, ResumptionPskStore>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<LeafNodeIndex>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .own_leaf_index::<GroupId, LeafNodeIndex>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupEpochSecrets>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .group_epoch_secrets::<GroupId, GroupEpochSecrets>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<Option<SignatureKeyPair>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .signature_key_pair::<SignaturePublicKey, SignatureKeyPair>(public_key)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn encryption_key_pair<
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<Option<HpkeKeyPair>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .encryption_key_pair::<HpkeKeyPair, EncryptionKey>(public_key)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
    ) -> Result<Vec<HpkeKeyPair>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .encryption_epoch_key_pairs::<GroupId, EpochKey, HpkeKeyPair>(group_id, epoch, leaf_index)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn key_package<
        KeyPackageRef: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &KeyPackageRef,
    ) -> Result<Option<KeyPackage>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .key_package::<KeyPackageRef, KeyPackage>(hash_ref)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn psk<
        PskBundle: traits::PskBundle<CURRENT_VERSION>,
        PskId: traits::PskId<CURRENT_VERSION>,
    >(
        &self,
        psk_id: &PskId,
    ) -> Result<Option<PskBundle>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .psk::<PskBundle, PskId>(psk_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn remove_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .remove_proposal::<GroupId, ProposalRef>(group_id, proposal_ref)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_own_leaf_nodes<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_own_leaf_nodes::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_group_config<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_group_config::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_tree<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_tree::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_confirmation_tag<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_confirmation_tag::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_group_state<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_group_state::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_context<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_context::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_interim_transcript_hash<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_interim_transcript_hash::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_message_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_message_secrets::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_all_resumption_psk_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_all_resumption_psk_secrets::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_own_leaf_index<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_own_leaf_index::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_group_epoch_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_group_epoch_secrets::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn clear_proposal_queue<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .clear_proposal_queue::<GroupId, ProposalRef>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_signature_key_pair::<SignaturePublicKey>(public_key)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_encryption_key_pair<EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>>(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_encryption_key_pair::<EncryptionKey>(public_key)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_encryption_epoch_key_pairs::<GroupId, EpochKey>(group_id, epoch, leaf_index)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_key_package<KeyPackageRef: traits::HashReference<CURRENT_VERSION>>(
        &self,
        hash_ref: &KeyPackageRef,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_key_package::<KeyPackageRef>(hash_ref)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_psk<PskKey: traits::PskId<CURRENT_VERSION>>(
        &self,
        psk_id: &PskKey,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_psk::<PskKey>(psk_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn write_application_export_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ApplicationExportTree: traits::ApplicationExportTree<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        application_export_tree: &ApplicationExportTree,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .write_application_export_tree::<GroupId, ApplicationExportTree>(group_id, application_export_tree)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn application_export_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ApplicationExportTree: traits::ApplicationExportTree<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ApplicationExportTree>, Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .application_export_tree::<GroupId, ApplicationExportTree>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_application_export_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ApplicationExportTree: traits::ApplicationExportTree<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<CborCodec>::new(&mut *conn)
            .delete_application_export_tree::<GroupId, ApplicationExportTree>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }
}

/// Transaction-scoped handle handed to a [`XmtpMlsStorageProvider::transaction`]
/// closure. Wraps the transaction-pinned [`PgDb`] so the closure can obtain a
/// key store that writes on that same pinned connection.
pub struct PgTxQuery {
    db: PgDb,
}

impl TransactionalKeyStore for PgTxQuery {
    type Store<'a>
        = PgKeyStore
    where
        Self: 'a;

    fn key_store<'a>(&'a mut self) -> Self::Store<'a> {
        PgKeyStore::new(self.db.clone())
    }
}

impl XmtpMlsStorageProvider for PgKeyStore {
    type Connection = PgDb;
    type TxQuery = PgTxQuery;
    type DbQuery<'a>
        = PgDb
    where
        Self::Connection: 'a;

    fn db<'a>(&'a self) -> Self::DbQuery<'a> {
        self.db.clone()
    }

    fn transaction<T, E, F>(
        &self,
        f: F,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + xmtp_common::MaybeSend
    where
        T: xmtp_common::MaybeSend,
        F: AsyncFnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>
            + TxFn<Self::TxQuery, T, E>,
        E: From<crate::ConnectionError> + std::error::Error + xmtp_common::MaybeSend,
    {
        async move {
        // `PgDb::transaction` commits on `Ok` and rolls back on `Err`. To roll
        // back *without* surfacing an error (the `Rollback` outcome) we return an
        // `Err` from the inner closure to trigger the rollback, then translate it
        // back to `Ok(Rollback)` here. The `Cell` distinguishes that intentional
        // rollback from a real error `f` returned.
        // AtomicBool (not Cell) so the future stays `Send`: the flag is borrowed
        // into the inner transaction closure and read after an `.await`.
        let rolled_back = std::sync::atomic::AtomicBool::new(false);
        let result = self
            .db
            .transaction(async |scoped: &PgDb| {
                let mut txq = PgTxQuery { db: scoped.clone() };
                match f.run(&mut txq).await {
                    Ok(TransactionOutcome::Continue(v)) => Ok(TransactionOutcome::Continue(v)),
                    Ok(TransactionOutcome::Rollback) => {
                        rolled_back.store(true, std::sync::atomic::Ordering::SeqCst);
                        Err(E::from(crate::ConnectionError::InvalidQuery(
                            "intentional rollback".into(),
                        )))
                    }
                    Err(e) => Err(e),
                }
            })
            .await;
        match result {
            Ok(o) => Ok(o),
            Err(_) if rolled_back.load(std::sync::atomic::Ordering::SeqCst) => {
                Ok(TransactionOutcome::Rollback)
            }
            Err(e) => Err(e),
        }
        }
    }

    // A savepoint is a `transaction` that nests: called inside an open
    // transaction it maps to a real Postgres SAVEPOINT (via `PgDb::savepoint`),
    // called standalone it degrades to a fresh transaction. The Rollback-outcome
    // translation is identical to `transaction` above — only the underlying
    // `PgDb` primitive differs.
    fn savepoint<T, E, F>(
        &self,
        f: F,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + xmtp_common::MaybeSend
    where
        T: xmtp_common::MaybeSend,
        F: AsyncFnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>
            + TxFn<Self::TxQuery, T, E>,
        E: From<crate::ConnectionError> + std::error::Error + xmtp_common::MaybeSend,
    {
        async move {
            let rolled_back = std::sync::atomic::AtomicBool::new(false);
            let result = self
                .db
                .savepoint(async |scoped: &PgDb| {
                    let mut txq = PgTxQuery { db: scoped.clone() };
                    match f.run(&mut txq).await {
                        Ok(TransactionOutcome::Continue(v)) => Ok(TransactionOutcome::Continue(v)),
                        Ok(TransactionOutcome::Rollback) => {
                            rolled_back.store(true, std::sync::atomic::Ordering::SeqCst);
                            Err(E::from(crate::ConnectionError::InvalidQuery(
                                "intentional rollback".into(),
                            )))
                        }
                        Err(e) => Err(e),
                    }
                })
                .await;
            match result {
                Ok(o) => Ok(o),
                Err(_) if rolled_back.load(std::sync::atomic::Ordering::SeqCst) => {
                    Ok(TransactionOutcome::Rollback)
                }
                Err(e) => Err(e),
            }
        }
    }

    // --- Generic (label, key, value) byte accessors ----------------------
    //
    // libxmtp's own raw KV interface (key package references, the commit-log
    // signer key, …), distinct from OpenMLS' typed `StorageProvider` methods
    // above. Backed by the Postgres `openmls_key_value` table (`label || key ||
    // version_be` key layout, bincode values). Genuinely async — each body takes
    // a connection from the handle and awaits sqlx, exactly like the typed
    // methods above.
    async fn read<V: Entity<CURRENT_VERSION> + xmtp_common::MaybeSend>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<Option<V>, SqlKeyStoreError> {
        let Some(bytes) = self.kv_select(label, key).await? else {
            return Ok(None);
        };
        Ok(Some(
            bincode::deserialize::<V>(&bytes).map_err(|_| SqlKeyStoreError::SerializationError)?,
        ))
    }

    async fn read_list<V: Entity<CURRENT_VERSION> + xmtp_common::MaybeSend>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<Vec<V>, <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        let Some(bytes) = self.kv_select(label, key).await? else {
            return Ok(vec![]);
        };
        // Stored as bincode(Vec<Vec<u8>>); each inner element is bincode(V).
        let list: Vec<Vec<u8>> =
            bincode::deserialize(&bytes).map_err(|_| SqlKeyStoreError::SerializationError)?;
        list.iter()
            .map(|item| {
                bincode::deserialize::<V>(item).map_err(|_| SqlKeyStoreError::SerializationError)
            })
            .collect()
    }

    async fn delete(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        let storage_key = build_kv_key(label, key);
        let mut c = self.db.conn().await?;
        sqlx::query("DELETE FROM openmls_key_value WHERE key_bytes = $1 AND version = $2")
            .bind(storage_key)
            .bind(CURRENT_VERSION as i32)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
        Ok(())
    }

    async fn write(
        &self,
        label: &[u8],
        key: &[u8],
        value: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        let storage_key = build_kv_key(label, key);
        let mut c = self.db.conn().await?;
        sqlx::query(
            "INSERT INTO openmls_key_value (key_bytes, version, value_bytes) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (version, key_bytes) DO UPDATE SET value_bytes = EXCLUDED.value_bytes",
        )
        .bind(storage_key)
        .bind(CURRENT_VERSION as i32)
        .bind(value.to_vec())
        .execute(&mut *c)
        .await
        .map_err(crate::ConnectionError::from)?;
        Ok(())
    }

    // TODO(pg-hash-all): hash the `openmls_*` tables for cross-track test parity.
    #[cfg(feature = "test-utils")]
    fn hash_all(&self) -> Result<Vec<u8>, SqlKeyStoreError> {
        Err(SqlKeyStoreError::UnsupportedMethod)
    }
}
