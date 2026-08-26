use std::sync::Arc;

use openmls_traits::storage::StorageProvider;
use openmls_traits::storage::{CURRENT_VERSION, traits};
use parking_lot::Mutex;

use crate::mock::{MockConnection, MockDbQuery};
use crate::{
    MemoryStorage,
    sql_key_store::{SqlKeyStore, SqlKeyStoreError},
};
use crate::{MockTransactionalKeyStore, TransactionOutcome, XmtpMlsStorageProvider};

/// An Mls provider that delegates MLS stuff to
/// in-memory sqlite store,
/// otherwise uses mockall
#[derive(Clone)]
pub struct MockSqlKeyStore {
    in_memory: Arc<SqlKeyStore<MemoryStorage>>,
    db_query: Arc<MockDbQuery>,
    pub mock_mls: Arc<Mutex<MockTransactionalKeyStore>>,
}

impl MockSqlKeyStore {
    pub fn mls(&self) -> &impl XmtpMlsStorageProvider {
        self.in_memory.as_ref()
    }
}

impl MockSqlKeyStore {
    pub fn new(
        db: Arc<MockDbQuery>,
        store: MockTransactionalKeyStore,
        mem: Arc<SqlKeyStore<MemoryStorage>>,
    ) -> Self {
        Self {
            db_query: db,
            in_memory: mem,
            mock_mls: Arc::new(Mutex::new(store)),
        }
    }
}

impl XmtpMlsStorageProvider for MockSqlKeyStore {
    type Connection = MockConnection;

    type DbQuery<'a> = &'a MockDbQuery;

    type TxQuery = MockTransactionalKeyStore;

    fn db<'a>(&'a self) -> Self::DbQuery<'a> {
        self.db_query.as_ref()
    }

    // Matches the `SqlKeyStore` sync impl shape: a bare `fn` returning
    // `impl Future + MaybeSend` (not a blocking `fn -> Result`), so the future is
    // `Send` for spawned tasks. The body is synchronous (drives `f` to completion
    // against the locked mock store) wrapped in a ready `async move`.
    #[tracing::instrument(level = "trace", skip_all)]
    fn transaction<T, E, F>(
        &self,
        f: F,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + xmtp_common::MaybeSend
    where
        T: xmtp_common::MaybeSend,
        F: AsyncFnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>
            + xmtp_common::MaybeSend,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        async move {
            let mut store = self.mock_mls.lock();
            super::transactions::drive_to_completion(f(&mut store))
        }
    }

    fn savepoint<T, E, F>(
        &self,
        f: F,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + xmtp_common::MaybeSend
    where
        T: xmtp_common::MaybeSend,
        F: AsyncFnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>
            + xmtp_common::MaybeSend,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        async move {
            let mut store = self.mock_mls.lock();
            super::transactions::drive_to_completion(f(&mut store))
        }
    }


    fn set_commit_log_signer_key(
        &self,
        group_id: &[u8],
        signer_key: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        XmtpMlsStorageProvider::set_commit_log_signer_key(
            self.in_memory.as_ref(),
            group_id,
            signer_key,
        )
    }

    fn commit_log_signer_key<V: openmls_traits::storage::Entity<CURRENT_VERSION> + xmtp_common::MaybeSend>(
        &self,
        group_id: &[u8],
    ) -> impl std::future::Future<Output = Result<Option<V>, SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        XmtpMlsStorageProvider::commit_log_signer_key::<V>(self.in_memory.as_ref(), group_id)
    }

    fn set_key_package_reference(
        &self,
        public_key: &[u8],
        hash_ref: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        XmtpMlsStorageProvider::set_key_package_reference(self.in_memory.as_ref(), public_key, hash_ref)
    }

    fn key_package_reference<V: openmls_traits::storage::Entity<CURRENT_VERSION> + xmtp_common::MaybeSend>(
        &self,
        public_key: &[u8],
    ) -> impl std::future::Future<Output = Result<Option<V>, SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        XmtpMlsStorageProvider::key_package_reference::<V>(self.in_memory.as_ref(), public_key)
    }

    fn delete_key_package_reference(
        &self,
        public_key: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        XmtpMlsStorageProvider::delete_key_package_reference(self.in_memory.as_ref(), public_key)
    }

    fn set_key_package_wrapper_key(
        &self,
        hash_ref: &[u8],
        private_key: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        XmtpMlsStorageProvider::set_key_package_wrapper_key(self.in_memory.as_ref(), hash_ref, private_key)
    }

    fn key_package_wrapper_key<V: openmls_traits::storage::Entity<CURRENT_VERSION> + xmtp_common::MaybeSend>(
        &self,
        hash_ref: &[u8],
    ) -> impl std::future::Future<Output = Result<Option<V>, SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        XmtpMlsStorageProvider::key_package_wrapper_key::<V>(self.in_memory.as_ref(), hash_ref)
    }

    fn delete_key_package_wrapper_key(
        &self,
        hash_ref: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        XmtpMlsStorageProvider::delete_key_package_wrapper_key(self.in_memory.as_ref(), hash_ref)
    }

    #[cfg(feature = "test-utils")]
    fn hash_all(&self) -> Result<Vec<u8>, SqlKeyStoreError> {
        self.in_memory.hash_all()
    }
}

#[maybe_async::maybe_async(AFIT)]
impl StorageProvider<CURRENT_VERSION> for MockSqlKeyStore {
    type Error = SqlKeyStoreError;

    #[tracing::instrument(level = "trace", skip_all)]
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
        self.in_memory
            .queue_proposal::<GroupId, ProposalRef, QueuedProposal>(
                group_id,
                proposal_ref,
                proposal,
            ).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        tree: &TreeSync,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_tree::<GroupId, TreeSync>(group_id, tree).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_interim_transcript_hash<
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
            ).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_context: &GroupContext,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_context::<GroupId, GroupContext>(group_id, group_context).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        confirmation_tag: &ConfirmationTag,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_confirmation_tag::<GroupId, ConfirmationTag>(group_id, confirmation_tag).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_signature_key_pair<
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
            ).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn queued_proposal_refs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<ProposalRef>, Self::Error> {
        self.in_memory
            .queued_proposal_refs::<GroupId, ProposalRef>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn queued_proposals<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<(ProposalRef, QueuedProposal)>, Self::Error> {
        self.in_memory
            .queued_proposals::<GroupId, ProposalRef, QueuedProposal>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<TreeSync>, Self::Error> {
        self.in_memory.tree::<GroupId, TreeSync>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn group_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupContext>, Self::Error> {
        self.in_memory
            .group_context::<GroupId, GroupContext>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<InterimTranscriptHash>, Self::Error> {
        self.in_memory
            .interim_transcript_hash::<GroupId, InterimTranscriptHash>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ConfirmationTag>, Self::Error> {
        self.in_memory
            .confirmation_tag::<GroupId, ConfirmationTag>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<Option<SignatureKeyPair>, Self::Error> {
        self.in_memory
            .signature_key_pair::<SignaturePublicKey, SignatureKeyPair>(public_key).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_key_package<
        HashReference: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &HashReference,
        key_package: &KeyPackage,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_key_package::<HashReference, KeyPackage>(hash_ref, key_package).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_psk<
        PskId: traits::PskId<CURRENT_VERSION>,
        PskBundle: traits::PskBundle<CURRENT_VERSION>,
    >(
        &self,
        _psk_id: &PskId,
        _psk: &PskBundle,
    ) -> Result<(), Self::Error> {
        self.in_memory.write_psk::<PskId, PskBundle>(_psk_id, _psk).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_encryption_key_pair<
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
        key_pair: &HpkeKeyPair,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_encryption_key_pair::<EncryptionKey, HpkeKeyPair>(public_key, key_pair).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn key_package<
        HashReference: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &HashReference,
    ) -> Result<Option<KeyPackage>, Self::Error> {
        self.in_memory
            .key_package::<HashReference, KeyPackage>(hash_ref).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn psk<PskBundle: traits::PskBundle<CURRENT_VERSION>, PskId: traits::PskId<CURRENT_VERSION>>(
        &self,
        _psk_id: &PskId,
    ) -> Result<Option<PskBundle>, Self::Error> {
        self.in_memory.psk::<PskBundle, PskId>(_psk_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn encryption_key_pair<
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<Option<HpkeKeyPair>, Self::Error> {
        self.in_memory
            .encryption_key_pair::<HpkeKeyPair, EncryptionKey>(public_key).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .delete_signature_key_pair::<SignaturePublicKey>(public_key).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_encryption_key_pair<EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>>(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .delete_encryption_key_pair::<EncryptionKey>(public_key).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_key_package<HashReference: traits::HashReference<CURRENT_VERSION>>(
        &self,
        hash_ref: &HashReference,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_key_package::<HashReference>(hash_ref).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_psk<PskKey: traits::PskId<CURRENT_VERSION>>(
        &self,
        _psk_id: &PskKey,
    ) -> Result<(), Self::Error> {
        Err(SqlKeyStoreError::UnsupportedMethod)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupState>, Self::Error> {
        self.in_memory.group_state::<GroupState, GroupId>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_state: &GroupState,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_group_state::<GroupState, GroupId>(group_id, group_state).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_group_state<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_group_state::<GroupId>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MessageSecrets>, Self::Error> {
        self.in_memory
            .message_secrets::<GroupId, MessageSecrets>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        message_secrets: &MessageSecrets,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_message_secrets::<GroupId, MessageSecrets>(group_id, message_secrets).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_message_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_message_secrets::<GroupId>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ResumptionPskStore>, Self::Error> {
        self.in_memory.resumption_psk_store(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_resumption_psk_store<
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
            ).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_all_resumption_psk_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_all_resumption_psk_secrets(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<LeafNodeIndex>, Self::Error> {
        self.in_memory.own_leaf_index(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        own_leaf_index: &LeafNodeIndex,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_own_leaf_index::<GroupId, LeafNodeIndex>(group_id, own_leaf_index).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_own_leaf_index<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_own_leaf_index::<GroupId>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupEpochSecrets>, Self::Error> {
        self.in_memory.group_epoch_secrets(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_epoch_secrets: &GroupEpochSecrets,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_group_epoch_secrets::<GroupId, GroupEpochSecrets>(group_id, group_epoch_secrets).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_group_epoch_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .delete_group_epoch_secrets::<GroupId>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
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
        self.in_memory
            .write_encryption_epoch_key_pairs(group_id, epoch, leaf_index, key_pairs).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
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
        self.in_memory
            .encryption_epoch_key_pairs(group_id, epoch, leaf_index).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .delete_encryption_epoch_key_pairs(group_id, epoch, leaf_index).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn clear_proposal_queue<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .clear_proposal_queue::<GroupId, ProposalRef>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn mls_group_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MlsGroupJoinConfig>, Self::Error> {
        self.in_memory
            .mls_group_join_config::<GroupId, MlsGroupJoinConfig>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn write_mls_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        config: &MlsGroupJoinConfig,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_mls_join_config::<GroupId, MlsGroupJoinConfig>(group_id, config).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn own_leaf_nodes<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<LeafNode>, Self::Error> {
        self.in_memory.own_leaf_nodes::<GroupId, LeafNode>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn append_own_leaf_node<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        leaf_node: &LeafNode,
    ) -> Result<(), Self::Error> {
        self.in_memory.append_own_leaf_node(group_id, leaf_node).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_own_leaf_nodes<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_own_leaf_nodes(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_group_config<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_group_config::<GroupId>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_tree<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_tree(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_confirmation_tag<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_confirmation_tag::<GroupId>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_context<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory.delete_context::<GroupId>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_interim_transcript_hash<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .delete_interim_transcript_hash::<GroupId>(group_id).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn remove_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .remove_proposal::<GroupId, ProposalRef>(group_id, proposal_ref).await
    }

    async fn write_application_export_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ApplicationExportTree: traits::ApplicationExportTree<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        application_export_tree: &ApplicationExportTree,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .write_application_export_tree::<GroupId, ApplicationExportTree>(
                group_id,
                application_export_tree,
            ).await
    }

    async fn application_export_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ApplicationExportTree: traits::ApplicationExportTree<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ApplicationExportTree>, Self::Error> {
        self.in_memory
            .application_export_tree::<GroupId, ApplicationExportTree>(group_id).await
    }

    async fn delete_application_export_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ApplicationExportTree: traits::ApplicationExportTree<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.in_memory
            .delete_application_export_tree::<GroupId, ApplicationExportTree>(group_id).await
    }
}
