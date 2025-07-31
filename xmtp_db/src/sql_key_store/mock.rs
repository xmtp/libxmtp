use std::marker::PhantomData;
use std::sync::Arc;

use mockall::mock;
use openmls_traits::storage::StorageProvider;
use openmls_traits::storage::{CURRENT_VERSION, traits};
use parking_lot::Mutex;

use crate::mock::{MockConnection, MockDbQuery};
use crate::{
    ConnectionExt, MemoryStorage,
    sql_key_store::{SqlKeyStore, SqlKeyStoreError},
};
use crate::{DbConnection, DbQuery, MlsKeyStore, MockMlsKeyStore, XmtpMlsStorageProvider};

/// An Mls provider that delegates MLS stuff to
/// in-memory sqlite store,
/// otherwise uses mockall
pub struct MockSqlKeyStore {
    in_memory: SqlKeyStore<MemoryStorage>,
    db_query: Arc<MockDbQuery>,
    mock_mls: Mutex<MockMlsKeyStore>
}

impl MockSqlKeyStore {
    pub fn new(db: MockDbQuery, store: MockMlsKeyStore) -> Self {
        Self {
            db_query: Arc::new(db),
            in_memory: SqlKeyStore::new(MemoryStorage::new()),
            mock_mls: Mutex::new(store)
        }
    }
}

impl StorageProvider<CURRENT_VERSION> for MockSqlKeyStore {
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
        self.in_memory
            .queue_proposal::<GroupId, ProposalRef, QueuedProposal>(
                group_id,
                proposal_ref,
                proposal,
            )
    }

    fn write_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        tree: &TreeSync,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_tree::<GroupId, TreeSync>(group_id, tree)
    }

    fn write_interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        interim_transcript_hash: &InterimTranscriptHash,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_interim_transcript_hash::<GroupId, InterimTranscriptHash>(
                group_id,
                interim_transcript_hash,
            )
    }

    fn write_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_context: &GroupContext,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_context::<GroupId, GroupContext>(group_id, group_context)
    }

    fn write_confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        confirmation_tag: &ConfirmationTag,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_confirmation_tag::<GroupId, ConfirmationTag>(group_id, confirmation_tag)
    }

    fn write_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
        signature_key_pair: &SignatureKeyPair,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_signature_key_pair::<SignaturePublicKey, SignatureKeyPair>(
                public_key,
                signature_key_pair,
            )
    }

    fn queued_proposal_refs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<ProposalRef>, Self::Error> {
        self.in_memory
            .queued_proposal_refs::<GroupId, ProposalRef>(group_id)
    }

    fn queued_proposals<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<(ProposalRef, QueuedProposal)>, Self::Error> {
        self.in_memory
            .queued_proposals::<GroupId, ProposalRef, QueuedProposal>(group_id)
    }

    fn tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<TreeSync>, Self::Error> {
        self.in_memory.tree::<GroupId, TreeSync>(group_id)
    }

    fn group_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupContext>, Self::Error> {
        self.in_memory
            .group_context::<GroupId, GroupContext>(group_id)
    }

    fn interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<InterimTranscriptHash>, Self::Error> {
        self.in_memory
            .interim_transcript_hash::<GroupId, InterimTranscriptHash>(group_id)
    }

    fn confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ConfirmationTag>, Self::Error> {
        self.in_memory
            .confirmation_tag::<GroupId, ConfirmationTag>(group_id)
    }

    fn signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<Option<SignatureKeyPair>, Self::Error> {
        self.in_memory
            .signature_key_pair::<SignaturePublicKey, SignatureKeyPair>(public_key)
    }

    fn write_key_package<
        HashReference: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &HashReference,
        key_package: &KeyPackage,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_key_package::<HashReference, KeyPackage>(hash_ref, key_package)
    }

    fn write_psk<
        PskId: traits::PskId<CURRENT_VERSION>,
        PskBundle: traits::PskBundle<CURRENT_VERSION>,
    >(
        &self,
        _psk_id: &PskId,
        _psk: &PskBundle,
    ) -> Result<(), Self::Error> {
        self.in_memory.write_psk::<PskId, PskBundle>(_psk_id, _psk)
    }

    fn write_encryption_key_pair<
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
        key_pair: &HpkeKeyPair,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_encryption_key_pair::<EncryptionKey, HpkeKeyPair>(public_key, key_pair)
    }

    fn key_package<
        HashReference: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &HashReference,
    ) -> Result<Option<KeyPackage>, Self::Error> {
        self.in_memory
            .key_package::<HashReference, KeyPackage>(hash_ref)
    }

    fn psk<PskBundle: traits::PskBundle<CURRENT_VERSION>, PskId: traits::PskId<CURRENT_VERSION>>(
        &self,
        _psk_id: &PskId,
    ) -> Result<Option<PskBundle>, Self::Error> {
        self.in_memory.psk::<PskBundle, PskId>(_psk_id)
    }

    fn encryption_key_pair<
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<Option<HpkeKeyPair>, Self::Error> {
        self.in_memory
            .encryption_key_pair::<HpkeKeyPair, EncryptionKey>(public_key)
    }

    fn delete_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .delete_signature_key_pair::<SignaturePublicKey>(public_key)
    }

    fn delete_encryption_key_pair<EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>>(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .delete_encryption_key_pair::<EncryptionKey>(public_key)
    }

    fn delete_key_package<HashReference: traits::HashReference<CURRENT_VERSION>>(
        &self,
        hash_ref: &HashReference,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_key_package::<HashReference>(hash_ref)
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
        self.in_memory.group_state::<GroupState, GroupId>(group_id)
    }

    fn write_group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_state: &GroupState,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_group_state::<GroupState, GroupId>(group_id, group_state)
    }

    fn delete_group_state<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_group_state::<GroupId>(group_id)
    }

    fn message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MessageSecrets>, Self::Error> {
        self.in_memory
            .message_secrets::<GroupId, MessageSecrets>(group_id)
    }

    fn write_message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        message_secrets: &MessageSecrets,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_message_secrets::<GroupId, MessageSecrets>(group_id, message_secrets)
    }

    fn delete_message_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_message_secrets::<GroupId>(group_id)
    }

    fn resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ResumptionPskStore>, Self::Error> {
        self.in_memory.resumption_psk_store(group_id)
    }

    fn write_resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        resumption_psk_store: &ResumptionPskStore,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_resumption_psk_store::<GroupId, ResumptionPskStore>(
                group_id,
                resumption_psk_store,
            )
    }

    fn delete_all_resumption_psk_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_all_resumption_psk_secrets(group_id)
    }

    fn own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<LeafNodeIndex>, Self::Error> {
        self.in_memory.own_leaf_index(group_id)
    }

    fn write_own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        own_leaf_index: &LeafNodeIndex,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_own_leaf_index::<GroupId, LeafNodeIndex>(group_id, own_leaf_index)
    }

    fn delete_own_leaf_index<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_own_leaf_index::<GroupId>(group_id)
    }

    fn group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupEpochSecrets>, Self::Error> {
        self.in_memory.group_epoch_secrets(group_id)
    }

    fn write_group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_epoch_secrets: &GroupEpochSecrets,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_group_epoch_secrets::<GroupId, GroupEpochSecrets>(group_id, group_epoch_secrets)
    }

    fn delete_group_epoch_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .delete_group_epoch_secrets::<GroupId>(group_id)
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
        self.in_memory
            .write_encryption_epoch_key_pairs(group_id, epoch, leaf_index, key_pairs)
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
        self.in_memory
            .encryption_epoch_key_pairs(group_id, epoch, leaf_index)
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
        self.in_memory
            .delete_encryption_epoch_key_pairs(group_id, epoch, leaf_index)
    }

    fn clear_proposal_queue<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .clear_proposal_queue::<GroupId, ProposalRef>(group_id)
    }

    fn mls_group_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MlsGroupJoinConfig>, Self::Error> {
        self.in_memory
            .mls_group_join_config::<GroupId, MlsGroupJoinConfig>(group_id)
    }

    fn write_mls_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        config: &MlsGroupJoinConfig,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_mls_join_config::<GroupId, MlsGroupJoinConfig>(group_id, config)
    }

    fn own_leaf_nodes<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<LeafNode>, Self::Error> {
        self.in_memory.own_leaf_nodes::<GroupId, LeafNode>(group_id)
    }

    fn append_own_leaf_node<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        leaf_node: &LeafNode,
    ) -> Result<(), Self::Error> {
        self.in_memory.append_own_leaf_node(group_id, leaf_node)
    }

    fn delete_own_leaf_nodes<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_own_leaf_nodes(group_id)
    }

    fn delete_group_config<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_group_config::<GroupId>(group_id)
    }

    fn delete_tree<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_tree(group_id)
    }

    fn delete_confirmation_tag<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_confirmation_tag::<GroupId>(group_id)
    }

    fn delete_context<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_context::<GroupId>(group_id)
    }

    fn delete_interim_transcript_hash<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .delete_interim_transcript_hash::<GroupId>(group_id)
    }

    fn remove_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .remove_proposal::<GroupId, ProposalRef>(group_id, proposal_ref)
    }
}


impl XmtpMlsStorageProvider for MockSqlKeyStore {
    type Connection = MockConnection;

    type DbQuery<'a> = &'a MockDbQuery;

    type TxQuery = MockMlsKeyStore;

    fn db<'a>(&'a self) -> Self::DbQuery<'a> {
        self.db_query.as_ref()
    }

    fn transaction<T, E, F>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::TxQuery) -> Result<T, E>,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        let mut store = self.mock_mls.lock();
        f(&mut store)
    }

    fn read<V: openmls_traits::storage::Entity<CURRENT_VERSION>>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<Option<V>, SqlKeyStoreError> {
        XmtpMlsStorageProvider::read::<V>(&self.in_memory, label, key)
    }

    fn read_list<V: openmls_traits::storage::Entity<CURRENT_VERSION>>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<Vec<V>, <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        XmtpMlsStorageProvider::read_list::<V>(&self.in_memory, label, key)
    }

    fn delete(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        self.in_memory.delete::<CURRENT_VERSION>(label, key)
    }

    fn write(
        &self,
        label: &[u8],
        key: &[u8],
        value: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        self.in_memory.write::<CURRENT_VERSION>(label, key, value)
    }
}
