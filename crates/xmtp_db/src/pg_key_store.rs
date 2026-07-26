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

/// The `bincode` codec used to (de)serialize every stored OpenMLS value.
///
/// The sync track also serializes with `bincode`, so both storage backends
/// agree on the byte encoding of entities.
#[derive(Default)]
pub struct BincodeCodec;

impl Codec for BincodeCodec {
    type Error = bincode::Error;

    fn to_vec<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, Self::Error> {
        bincode::serialize(value)
    }

    fn from_slice<T: DeserializeOwned>(slice: &[u8]) -> Result<T, Self::Error> {
        bincode::deserialize(slice)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .remove_proposal::<GroupId, ProposalRef>(group_id, proposal_ref)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_own_leaf_nodes<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .delete_own_leaf_nodes::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_group_config<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .delete_group_config::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_tree<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .delete_tree::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_confirmation_tag<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .delete_confirmation_tag::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_group_state<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .delete_group_state::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_context<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .delete_context::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_interim_transcript_hash<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .delete_interim_transcript_hash::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_message_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .delete_message_secrets::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_all_resumption_psk_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .delete_all_resumption_psk_secrets::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_own_leaf_index<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .delete_own_leaf_index::<GroupId>(group_id)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_group_epoch_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .delete_signature_key_pair::<SignaturePublicKey>(public_key)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_encryption_key_pair<EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>>(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .delete_encryption_epoch_key_pairs::<GroupId, EpochKey>(group_id, epoch, leaf_index)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_key_package<KeyPackageRef: traits::HashReference<CURRENT_VERSION>>(
        &self,
        hash_ref: &KeyPackageRef,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
            .delete_key_package::<KeyPackageRef>(hash_ref)
            .await
            .map_err(|e| SqlKeyStoreError::from(crate::ConnectionError::from(e)))
    }

    async fn delete_psk<PskKey: traits::PskId<CURRENT_VERSION>>(
        &self,
        psk_id: &PskKey,
    ) -> Result<(), Self::Error> {
        let mut conn = self.db.conn().await?;
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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
        PostgresStorageProvider::<BincodeCodec>::new(&mut *conn)
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

    // TODO(savepoint): real Postgres SAVEPOINT nesting. For now a savepoint runs
    // through the same path as `transaction`; the async provider does not yet
    // model nested savepoints, and this keeps the shape correct until it does.
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
        async move { self.transaction(f).await }
    }

    // --- Generic (label, key, value) byte accessors ----------------------
    //
    // The sync track backs these with its `openmls_key_value` KV table. The
    // async Postgres schema has no generic KV table -- every OpenMLS value lands
    // in a typed `openmls_*` table via the `StorageProvider` methods above -- so
    // there is nothing to delegate to. `UnsupportedMethod` keeps the async build
    // honest rather than silently no-op'ing.
    // TODO(pg-kv): back read/read_list/delete/write with a generic KV table if a
    // caller on the async track needs the raw byte interface.
    fn read<V: Entity<CURRENT_VERSION>>(
        &self,
        _label: &[u8],
        _key: &[u8],
    ) -> Result<Option<V>, SqlKeyStoreError> {
        Err(SqlKeyStoreError::UnsupportedMethod)
    }

    fn read_list<V: Entity<CURRENT_VERSION>>(
        &self,
        _label: &[u8],
        _key: &[u8],
    ) -> Result<Vec<V>, <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        Err(SqlKeyStoreError::UnsupportedMethod)
    }

    fn delete(
        &self,
        _label: &[u8],
        _key: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        Err(SqlKeyStoreError::UnsupportedMethod)
    }

    fn write(
        &self,
        _label: &[u8],
        _key: &[u8],
        _value: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        Err(SqlKeyStoreError::UnsupportedMethod)
    }

    // TODO(pg-hash-all): hash the `openmls_*` tables for cross-track test parity.
    #[cfg(feature = "test-utils")]
    fn hash_all(&self) -> Result<Vec<u8>, SqlKeyStoreError> {
        Err(SqlKeyStoreError::UnsupportedMethod)
    }
}
