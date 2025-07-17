use openmls_memory_storage::{MemoryStorage as MlsMemoryStorage, MemoryStorageError};
use openmls_traits::storage::CURRENT_VERSION;
use openmls_traits::storage::StorageProvider;

use crate::sql_key_store::SqlKeyStoreError;

pub struct MemoryStorage {
    inner: MlsMemoryStorage,
}

impl StorageProvider<CURRENT_VERSION> for MemoryStorage {
    type Error = SqlKeyStoreError;

    fn write_mls_join_config<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: openmls_traits::storage::traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        config: &MlsGroupJoinConfig,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_mls_join_config(group_id, config)
            .map_err(SqlKeyStoreError::from)
    }

    fn append_own_leaf_node<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        LeafNode: openmls_traits::storage::traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        leaf_node: &LeafNode,
    ) -> Result<(), Self::Error> {
        self.inner
            .append_own_leaf_node(group_id, leaf_node)
            .map_err(SqlKeyStoreError::from)
    }

    fn queue_proposal<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        ProposalRef: openmls_traits::storage::traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: openmls_traits::storage::traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
        proposal: &QueuedProposal,
    ) -> Result<(), Self::Error> {
        self.inner
            .queue_proposal(group_id, proposal_ref, proposal)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_tree<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        TreeSync: openmls_traits::storage::traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        tree: &TreeSync,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_tree(group_id, tree)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_interim_transcript_hash<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: openmls_traits::storage::traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        interim_transcript_hash: &InterimTranscriptHash,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_interim_transcript_hash(group_id, interim_transcript_hash)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_context<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        GroupContext: openmls_traits::storage::traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_context: &GroupContext,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_context(group_id, group_context)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_confirmation_tag<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: openmls_traits::storage::traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        confirmation_tag: &ConfirmationTag,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_confirmation_tag(group_id, confirmation_tag)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_group_state<
        GroupState: openmls_traits::storage::traits::GroupState<CURRENT_VERSION>,
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_state: &GroupState,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_group_state(group_id, group_state)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_message_secrets<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: openmls_traits::storage::traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        message_secrets: &MessageSecrets,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_message_secrets(group_id, message_secrets)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_resumption_psk_store<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: openmls_traits::storage::traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        resumption_psk_store: &ResumptionPskStore,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_resumption_psk_store(group_id, resumption_psk_store)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_own_leaf_index<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: openmls_traits::storage::traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        own_leaf_index: &LeafNodeIndex,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_own_leaf_index(group_id, own_leaf_index)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_group_epoch_secrets<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: openmls_traits::storage::traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_epoch_secrets: &GroupEpochSecrets,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_group_epoch_secrets(group_id, group_epoch_secrets)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_signature_key_pair<
        SignaturePublicKey: openmls_traits::storage::traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: openmls_traits::storage::traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
        signature_key_pair: &SignatureKeyPair,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_signature_key_pair(public_key, signature_key_pair)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_encryption_key_pair<
        EncryptionKey: openmls_traits::storage::traits::EncryptionKey<CURRENT_VERSION>,
        HpkeKeyPair: openmls_traits::storage::traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
        key_pair: &HpkeKeyPair,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_encryption_key_pair(public_key, key_pair)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_encryption_epoch_key_pairs<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        EpochKey: openmls_traits::storage::traits::EpochKey<CURRENT_VERSION>,
        HpkeKeyPair: openmls_traits::storage::traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
        key_pairs: &[HpkeKeyPair],
    ) -> Result<(), Self::Error> {
        self.inner
            .write_encryption_epoch_key_pairs(group_id, epoch, leaf_index, key_pairs)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_key_package<
        HashReference: openmls_traits::storage::traits::HashReference<CURRENT_VERSION>,
        KeyPackage: openmls_traits::storage::traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &HashReference,
        key_package: &KeyPackage,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_key_package(hash_ref, key_package)
            .map_err(SqlKeyStoreError::from)
    }

    fn write_psk<
        PskId: openmls_traits::storage::traits::PskId<CURRENT_VERSION>,
        PskBundle: openmls_traits::storage::traits::PskBundle<CURRENT_VERSION>,
    >(
        &self,
        psk_id: &PskId,
        psk: &PskBundle,
    ) -> Result<(), Self::Error> {
        self.inner
            .write_psk(psk_id, psk)
            .map_err(SqlKeyStoreError::from)
    }

    fn mls_group_join_config<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: openmls_traits::storage::traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MlsGroupJoinConfig>, Self::Error> {
        self.inner
            .mls_group_join_config(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn own_leaf_nodes<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        LeafNode: openmls_traits::storage::traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<LeafNode>, Self::Error> {
        self.inner
            .own_leaf_nodes(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn queued_proposal_refs<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        ProposalRef: openmls_traits::storage::traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<ProposalRef>, Self::Error> {
        self.inner
            .queued_proposal_refs(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn queued_proposals<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        ProposalRef: openmls_traits::storage::traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: openmls_traits::storage::traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<(ProposalRef, QueuedProposal)>, Self::Error> {
        self.inner
            .queued_proposals(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn tree<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        TreeSync: openmls_traits::storage::traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<TreeSync>, Self::Error> {
        self.inner.tree(group_id).map_err(SqlKeyStoreError::from)
    }

    fn group_context<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        GroupContext: openmls_traits::storage::traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupContext>, Self::Error> {
        self.inner
            .group_context(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn interim_transcript_hash<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: openmls_traits::storage::traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<InterimTranscriptHash>, Self::Error> {
        self.inner
            .interim_transcript_hash(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn confirmation_tag<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: openmls_traits::storage::traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ConfirmationTag>, Self::Error> {
        self.inner
            .confirmation_tag(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn group_state<
        GroupState: openmls_traits::storage::traits::GroupState<CURRENT_VERSION>,
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupState>, Self::Error> {
        self.inner
            .group_state(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn message_secrets<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: openmls_traits::storage::traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MessageSecrets>, Self::Error> {
        self.inner
            .message_secrets(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn resumption_psk_store<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: openmls_traits::storage::traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ResumptionPskStore>, Self::Error> {
        self.inner
            .resumption_psk_store(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn own_leaf_index<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: openmls_traits::storage::traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<LeafNodeIndex>, Self::Error> {
        self.inner
            .own_leaf_index(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn group_epoch_secrets<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: openmls_traits::storage::traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupEpochSecrets>, Self::Error> {
        self.inner
            .group_epoch_secrets(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn signature_key_pair<
        SignaturePublicKey: openmls_traits::storage::traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: openmls_traits::storage::traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<Option<SignatureKeyPair>, Self::Error> {
        self.inner
            .signature_key_pair(public_key)
            .map_err(SqlKeyStoreError::from)
    }

    fn encryption_key_pair<
        HpkeKeyPair: openmls_traits::storage::traits::HpkeKeyPair<CURRENT_VERSION>,
        EncryptionKey: openmls_traits::storage::traits::EncryptionKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<Option<HpkeKeyPair>, Self::Error> {
        self.inner
            .encryption_key_pair(public_key)
            .map_err(SqlKeyStoreError::from)
    }

    fn encryption_epoch_key_pairs<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        EpochKey: openmls_traits::storage::traits::EpochKey<CURRENT_VERSION>,
        HpkeKeyPair: openmls_traits::storage::traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
    ) -> Result<Vec<HpkeKeyPair>, Self::Error> {
        self.inner
            .encryption_epoch_key_pairs(group_id, epoch, leaf_index)
            .map_err(SqlKeyStoreError::from)
    }

    fn key_package<
        KeyPackageRef: openmls_traits::storage::traits::HashReference<CURRENT_VERSION>,
        KeyPackage: openmls_traits::storage::traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &KeyPackageRef,
    ) -> Result<Option<KeyPackage>, Self::Error> {
        self.inner
            .key_package(hash_ref)
            .map_err(SqlKeyStoreError::from)
    }

    fn psk<
        PskBundle: openmls_traits::storage::traits::PskBundle<CURRENT_VERSION>,
        PskId: openmls_traits::storage::traits::PskId<CURRENT_VERSION>,
    >(
        &self,
        psk_id: &PskId,
    ) -> Result<Option<PskBundle>, Self::Error> {
        self.inner.psk(psk_id).map_err(SqlKeyStoreError::from)
    }

    fn remove_proposal<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        ProposalRef: openmls_traits::storage::traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
    ) -> Result<(), Self::Error> {
        self.inner
            .remove_proposal(group_id, proposal_ref)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_own_leaf_nodes<GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_own_leaf_nodes(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_group_config<GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_group_config(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_tree<GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_tree(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_confirmation_tag<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_confirmation_tag(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_group_state<GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_group_state(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_context<GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_context(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_interim_transcript_hash<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_interim_transcript_hash(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_message_secrets<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_message_secrets(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_all_resumption_psk_secrets<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_all_resumption_psk_secrets(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_own_leaf_index<GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_own_leaf_index(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_group_epoch_secrets<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_group_epoch_secrets(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn clear_proposal_queue<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        ProposalRef: openmls_traits::storage::traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.inner
            .clear_proposal_queue::<GroupId, ProposalRef>(group_id)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_signature_key_pair<
        SignaturePublicKey: openmls_traits::storage::traits::SignaturePublicKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_signature_key_pair(public_key)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_encryption_key_pair<
        EncryptionKey: openmls_traits::storage::traits::EncryptionKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_encryption_key_pair(public_key)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_encryption_epoch_key_pairs<
        GroupId: openmls_traits::storage::traits::GroupId<CURRENT_VERSION>,
        EpochKey: openmls_traits::storage::traits::EpochKey<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_encryption_epoch_key_pairs(group_id, epoch, leaf_index)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_key_package<
        KeyPackageRef: openmls_traits::storage::traits::HashReference<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &KeyPackageRef,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_key_package(hash_ref)
            .map_err(SqlKeyStoreError::from)
    }

    fn delete_psk<PskKey: openmls_traits::storage::traits::PskId<CURRENT_VERSION>>(
        &self,
        psk_id: &PskKey,
    ) -> Result<(), Self::Error> {
        self.inner
            .delete_psk(psk_id)
            .map_err(SqlKeyStoreError::from)
    }
}

impl From<MemoryStorageError> for SqlKeyStoreError {
    fn from(value: MemoryStorageError) -> Self {
        match value {
            MemoryStorageError::UnsupportedValueTypeBytes => {
                SqlKeyStoreError::UnsupportedValueTypeBytes
            }
            MemoryStorageError::UnsupportedMethod => SqlKeyStoreError::UnsupportedMethod,
            MemoryStorageError::SerializationError => SqlKeyStoreError::SerializationError,
        }
    }
}
