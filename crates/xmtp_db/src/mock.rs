use crate::StorageError;
use crate::association_state::QueryAssociationStateCache;
use crate::group::ConversationType;
use crate::group::StoredGroupCommitLogPublicKey;
use crate::group_message::StoredGroupMessage;
use crate::local_commit_log::{LocalCommitLog, LocalCommitLogOrder};
use crate::remote_commit_log::{RemoteCommitLog, RemoteCommitLogOrder};
use std::collections::HashMap;
use std::sync::Arc;
use xmtp_proto::types::{Cursor, GlobalCursor, OrphanedEnvelope, Topic};
use xmtp_proto::xmtp::identity::associations::AssociationState as AssociationStateProto;

use crate::SqliteConnection;
use crate::prelude::*;
use mockall::mock;
use parking_lot::Mutex;

use crate::pending_remove::QueryPendingRemove;
use crate::{ConnectionError, ConnectionExt};

pub type MockDb = MockDbQuery;

#[derive(Clone)]
pub struct MockConnection {
    inner: Arc<Mutex<SqliteConnection>>,
}

impl std::fmt::Debug for MockConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MockConnection")
    }
}

impl AsRef<MockConnection> for MockConnection {
    fn as_ref(&self) -> &MockConnection {
        self
    }
}

// TODO: We should use diesels test transaction
impl ConnectionExt for MockConnection {
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.inner.lock();
        fun(&mut conn).map_err(ConnectionError::from)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.inner.lock();
        fun(&mut conn).map_err(ConnectionError::from)
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        Ok(())
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        Ok(())
    }
}

mock! {
    pub DbQuery {

    }

    impl ReadOnly for DbQuery {
        fn enable_readonly(&self) -> Result<(), StorageError>;
        fn disable_readonly(&self) -> Result<(), StorageError>;
    }

    impl QueryConsentRecord for DbQuery {
        fn get_consent_record(
            &self,
            entity: String,
            entity_type: crate::consent_record::ConsentType,
        ) -> Result<Option<crate::consent_record::StoredConsentRecord>, crate::ConnectionError>;

        fn consent_records(
            &self,
        ) -> Result<Vec<crate::consent_record::StoredConsentRecord>, crate::ConnectionError>;

        fn consent_records_paged(
            &self,
            limit: i64,
            offset: i64,
        ) -> Result<Vec<crate::consent_record::StoredConsentRecord>, crate::ConnectionError>;

        fn insert_newer_consent_record(
            &self,
            record: crate::consent_record::StoredConsentRecord,
        ) -> Result<bool, crate::ConnectionError>;

        fn insert_or_replace_consent_records(
            &self,
            records: &[crate::consent_record::StoredConsentRecord],
        ) -> Result<Vec<crate::consent_record::StoredConsentRecord>, crate::ConnectionError>;

        fn maybe_insert_consent_record_return_existing(
            &self,
            record: &crate::consent_record::StoredConsentRecord,
        ) -> Result<Option<crate::consent_record::StoredConsentRecord>, crate::ConnectionError>;

        fn find_consent_by_dm_id(
            &self,
            dm_id: &str,
        ) -> Result<Vec<crate::consent_record::StoredConsentRecord>, crate::ConnectionError>;
    }

    impl QueryConversationList for DbQuery {
        #[mockall::concretize]
        fn fetch_conversation_list<A: AsRef<crate::group::GroupQueryArgs>>(
            &self,
            args: A,
        ) -> Result<Vec<crate::conversation_list::ConversationListItem>, StorageError>;
    }

    impl QueryDms for DbQuery {
        fn fetch_stitched(
            &self,
            key: &[u8],
        ) -> Result<Option<crate::group::StoredGroup>, ConnectionError>;

        #[mockall::concretize]
        fn find_active_dm_group<M>(
            &self,
            members: M,
        ) -> Result<Option<crate::group::StoredGroup>, ConnectionError>
        where
            M: std::fmt::Display;

        fn other_dms(&self, group_id: &[u8])
        -> Result<Vec<crate::group::StoredGroup>, ConnectionError>;
    }

    impl QueryGroup for DbQuery {
        #[mockall::concretize]
        fn find_groups<A: AsRef<crate::group::GroupQueryArgs>>(
            &self,
            args: A,
        ) -> Result<Vec<crate::group::StoredGroup>, crate::ConnectionError>;

        #[mockall::concretize]
        fn find_groups_by_id_paged<A: AsRef<crate::group::GroupQueryArgs>>(
            &self,
            args: A,
            offset: i64,
        ) -> Result<Vec<crate::group::StoredGroup>, crate::ConnectionError>;

        #[mockall::concretize]
        fn update_group_membership<GroupId: AsRef<[u8]>>(
            &self,
            group_id: GroupId,
            state: crate::group::GroupMembershipState,
        ) -> Result<(), crate::ConnectionError>;

        fn all_sync_groups(&self) -> Result<Vec<crate::group::StoredGroup>, crate::ConnectionError>;

        fn find_sync_group(
            &self,
            id: &[u8],
        ) -> Result<Option<crate::group::StoredGroup>, crate::ConnectionError>;

        fn primary_sync_group(
            &self,
        ) -> Result<Option<crate::group::StoredGroup>, crate::ConnectionError>;

        fn find_group(
            &self,
            id: &[u8],
        ) -> Result<Option<crate::group::StoredGroup>, crate::ConnectionError>;

        fn find_group_by_sequence_id(
            &self,
            cursor: Cursor,
        ) -> Result<Option<crate::group::StoredGroup>, crate::ConnectionError>;

        fn get_rotated_at_ns(&self, group_id: Vec<u8>) -> Result<i64, StorageError>;

        fn update_rotated_at_ns(&self, group_id: Vec<u8>) -> Result<(), StorageError>;

        fn get_installations_time_checked(&self, group_id: Vec<u8>) -> Result<i64, StorageError>;

        fn update_installations_time_checked(&self, group_id: Vec<u8>) -> Result<(), StorageError>;

        fn update_message_disappearing_from_ns(
            &self,
            group_id: Vec<u8>,
            from_ns: Option<i64>,
        ) -> Result<(), StorageError>;

        fn update_message_disappearing_in_ns(
            &self,
            group_id: Vec<u8>,
            in_ns: Option<i64>,
        ) -> Result<(), StorageError>;

        fn insert_or_replace_group(
            &self,
            group: crate::group::StoredGroup,
        ) -> Result<crate::group::StoredGroup, StorageError>;

        fn group_cursors(&self) -> Result<Vec<Cursor>, crate::ConnectionError>;

        fn mark_group_as_maybe_forked(
            &self,
            group_id: &[u8],
            fork_details: String,
        ) -> Result<(), StorageError>;

        fn clear_fork_flag_for_group(&self, group_id: &[u8]) -> Result<(), crate::ConnectionError>;

        fn has_duplicate_dm(&self, group_id: &[u8]) -> Result<bool, crate::ConnectionError>;

        fn get_conversation_ids_for_remote_log_publish(&self) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError>;

        fn get_conversation_ids_for_remote_log_download(&self) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError>;

        fn get_conversation_ids_for_fork_check(
            &self,
        ) -> Result<Vec<Vec<u8>>, crate::ConnectionError>;

        fn get_conversation_ids_for_requesting_readds(
            &self,
        ) -> Result<Vec<crate::encrypted_store::group::StoredGroupForReaddRequest>, crate::ConnectionError>;

        fn get_conversation_ids_for_responding_readds(
            &self,
        ) -> Result<Vec<crate::encrypted_store::group::StoredGroupForRespondingReadds>, crate::ConnectionError>;

        fn get_conversation_type(&self, group_id: &[u8]) -> Result<ConversationType, crate::ConnectionError>;

        fn set_group_commit_log_public_key(
            &self,
            group_id: &[u8],
            public_key: &[u8],
        ) -> Result<(), StorageError>;

        fn set_group_commit_log_forked_status(
            &self,
            group_id: &[u8],
            is_forked: Option<bool>,
        ) -> Result<(), StorageError>;

        fn get_group_commit_log_forked_status(
            &self,
            group_id: &[u8],
        ) -> Result<Option<bool>, StorageError>;

        fn set_group_has_pending_leave_request_status(
            &self,
            group_id: &[u8],
            has_pending_leave_request: Option<bool>,
        ) -> Result<(), StorageError>;
            fn get_groups_have_pending_leave_request(
        &self,
    ) -> Result<Vec<Vec<u8>>, crate::ConnectionError>;
    }

    impl QueryGroupVersion for DbQuery {
        fn set_group_paused(&self, group_id: &[u8], min_version: &str) -> Result<(), StorageError>;

        fn unpause_group(&self, group_id: &[u8]) -> Result<(), StorageError>;

        fn get_group_paused_version(&self, group_id: &[u8]) -> Result<Option<String>, StorageError>;
    }

    impl QueryGroupIntent for DbQuery {
        fn insert_group_intent(
            &self,
            to_save: crate::group_intent::NewGroupIntent,
        ) -> Result<crate::group_intent::StoredGroupIntent, crate::ConnectionError>;

        #[mockall::concretize]
        fn find_group_intents<Id: AsRef<[u8]>>(
            &self,
            group_id: Id,
            allowed_states: Option<Vec<crate::group_intent::IntentState>>,
            allowed_kinds: Option<Vec<crate::group_intent::IntentKind>>,
        ) -> Result<Vec<crate::group_intent::StoredGroupIntent>, crate::ConnectionError>;

        fn set_group_intent_published(
            &self,
            intent_id: crate::group_intent::ID,
            payload_hash: &[u8],
            post_commit_data: Option<Vec<u8>>,
            staged_commit: Option<Vec<u8>>,
            published_in_epoch: i64,
        ) -> Result<(), StorageError>;

        fn set_group_intent_committed(
            &self,
            intent_id: crate::group_intent::ID,
            cursor: Cursor,
        ) -> Result<(), StorageError>;

        fn set_group_intent_processed(
            &self,
            intent_id: crate::group_intent::ID,
        ) -> Result<(), StorageError>;

        fn set_group_intent_to_publish(
            &self,
            intent_id: crate::group_intent::ID,
        ) -> Result<(), StorageError>;

        fn set_group_intent_error(
            &self,
            intent_id: crate::group_intent::ID,
        ) -> Result<(), StorageError>;

        fn find_group_intent_by_payload_hash(
            &self,
            payload_hash: &[u8],
        ) -> Result<Option<crate::group_intent::StoredGroupIntent>, StorageError>;

        #[mockall::concretize]
        fn find_dependant_commits<P: AsRef<[u8]>>(
            &self,
            payload_hashes: &[P],
        ) -> Result<HashMap<crate::group_intent::PayloadHash, crate::group_intent::IntentDependency>, StorageError>;

        fn increment_intent_publish_attempt_count(
            &self,
            intent_id: crate::group_intent::ID,
        ) -> Result<(), StorageError>;

        fn set_group_intent_error_and_fail_msg(
            &self,
            intent: &crate::group_intent::StoredGroupIntent,
            msg_id: Option<Vec<u8>>,
        ) -> Result<(), StorageError>;
    }

    impl QueryReaddStatus for DbQuery {
        fn get_readd_status(
            &self,
            group_id: &[u8],
            installation_id: &[u8],
        ) -> Result<Option<crate::readd_status::ReaddStatus>, crate::ConnectionError>;

        fn is_awaiting_readd(
            &self,
            group_id: &[u8],
            installation_id: &[u8],
        ) -> Result<bool, crate::ConnectionError>;

        fn update_requested_at_sequence_id(
            &self,
            group_id: &[u8],
            installation_id: &[u8],
            sequence_id: i64,
        ) -> Result<(), crate::ConnectionError>;

        fn update_responded_at_sequence_id(
            &self,
            group_id: &[u8],
            installation_id: &[u8],
            sequence_id: i64,
        ) -> Result<(), crate::ConnectionError>;

        fn delete_other_readd_statuses(
            &self,
            group_id: &[u8],
            self_installation_id: &[u8],
        ) -> Result<(), crate::ConnectionError>;

        fn delete_readd_statuses(
            &self,
            group_id: &[u8],
            installation_ids: std::collections::HashSet<Vec<u8> > ,
        ) -> Result<(), crate::ConnectionError>;

        fn get_readds_awaiting_response(
            &self,
            group_id: &[u8],
            self_installation_id: &[u8],
        ) -> Result<Vec<crate::readd_status::ReaddStatus>, crate::ConnectionError>;
    }

    impl QueryGroupMessage for DbQuery {
        fn get_group_messages(
            &self,
            group_id: &[u8],
            args: &crate::group_message::MsgQueryArgs,
        ) -> Result<Vec<crate::group_message::StoredGroupMessage>, crate::ConnectionError>;

        fn count_group_messages(
            &self,
            group_id: &[u8],
            args: &crate::group_message::MsgQueryArgs,
        ) -> Result<i64, crate::ConnectionError>;

        fn group_messages_paged(
            &self,
            args: &crate::group_message::MsgQueryArgs,
            offset: i64,
        ) -> Result<Vec<crate::group_message::StoredGroupMessage>, crate::ConnectionError>;

        fn get_group_messages_with_reactions(
            &self,
            group_id: &[u8],
            args: &crate::group_message::MsgQueryArgs,
        ) -> Result<Vec<crate::group_message::StoredGroupMessageWithReactions>, crate::ConnectionError>;

        fn get_inbound_relations<'a>(
            &self,
            group_id: &'a [u8],
            message_ids: &'a [&'a [u8]],
            relation_query: crate::group_message::RelationQuery,
        ) -> Result<crate::group_message::InboundRelations, crate::ConnectionError>;

        fn get_outbound_relations<'a>(
            &self,
            group_id: &'a [u8],
            message_ids: &'a [&'a [u8]],
        ) -> Result<crate::group_message::OutboundRelations, crate::ConnectionError>;

        fn get_inbound_relation_counts<'a>(
            &self,
            group_id: &'a [u8],
            message_ids: &'a [&'a [u8]],
            relation_query: crate::group_message::RelationQuery,
        ) -> Result<crate::group_message::RelationCounts, crate::ConnectionError>;

        #[mockall::concretize]
        fn get_group_message<MessageId: AsRef<[u8]>>(
            &self,
            id: MessageId,
        ) -> Result<Option<crate::group_message::StoredGroupMessage>, crate::ConnectionError>;

        #[mockall::concretize]
        fn write_conn_get_group_message<MessageId: AsRef<[u8]>>(
            &self,
            id: MessageId,
        ) -> Result<Option<crate::group_message::StoredGroupMessage>, crate::ConnectionError>;

        #[mockall::concretize]
        fn get_group_message_by_timestamp<GroupId: AsRef<[u8]>>(
            &self,
            group_id: GroupId,
            timestamp: i64,
        ) -> Result<Option<crate::group_message::StoredGroupMessage>, crate::ConnectionError>;

        #[mockall::concretize]
        fn get_group_message_by_cursor<GroupId: AsRef<[u8]>>(
            &self,
            group_id: GroupId,
            sequence_id: Cursor,
        ) -> Result<Option<crate::group_message::StoredGroupMessage>, crate::ConnectionError>;

        #[mockall::concretize]
        fn set_delivery_status_to_published<MessageId: AsRef<[u8]>>(
            &self,
            msg_id: &MessageId,
            timestamp: u64,
            cursor: Cursor,
            message_expire_at_ns: Option<i64>
        ) -> Result<usize, crate::ConnectionError>;

        #[mockall::concretize]
        fn set_delivery_status_to_failed<MessageId: AsRef<[u8]>>(
            &self,
            msg_id: &MessageId,
        ) -> Result<usize, crate::ConnectionError>;

        fn delete_expired_messages(&self) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError>;

        #[mockall::concretize]
        fn delete_message_by_id<MessageId: AsRef<[u8]>>(
            &self,
            message_id: MessageId,
        ) -> Result<usize, crate::ConnectionError>;

        #[mockall::concretize]
        fn get_latest_message_times_by_sender<GroupId: AsRef<[u8]>>(
            &self,
            group_id: GroupId,
            allowed_content_types: &[crate::group_message::ContentType],
        ) -> Result<crate::group_message::LatestMessageTimeBySender, crate::ConnectionError>;

        fn messages_newer_than(
            &self,
            cursors_by_group: &HashMap<Vec<u8>, xmtp_proto::types::GlobalCursor>,
        ) -> Result<Vec<Cursor>, crate::ConnectionError>;

        fn clear_messages<'a>(
            &self,
            group_ids: Option<&'a [Vec<u8>]>,
            retention_days: Option<u32>,
        ) -> Result<usize, crate::ConnectionError>;
    }

    impl QueryIdentity for DbQuery {
        fn queue_key_package_rotation(&self) -> Result<(), StorageError>;

        fn reset_key_package_rotation_queue(&self, rotation_interval: i64) -> Result<(), StorageError>;

        fn is_identity_needs_rotation(&self) -> Result<bool, StorageError>;
    }

    impl QueryIdentityCache for DbQuery {
        #[mockall::concretize]
        fn fetch_cached_inbox_ids<T>(
            &self,
            identifiers: &[T],
        ) -> Result<std::collections::HashMap<String, String>, StorageError>
        where
            T: std::fmt::Display,
            for<'a> &'a T: Into<crate::identity_cache::StoredIdentityKind>;

        #[mockall::concretize]
        fn cache_inbox_id<T, S>(
            &self,
            identifier: &T,
            inbox_id: S,
        ) -> Result<(), StorageError>
        where
            T: std::fmt::Display,
            S: ToString,
            for<'a> &'a T: Into<crate::identity_cache::StoredIdentityKind>;
    }

    impl QueryKeyPackageHistory for DbQuery {
        fn store_key_package_history_entry(
            &self,
            key_package_hash_ref: Vec<u8>,
            post_quantum_public_key: Option<Vec<u8>>,
        ) -> Result<crate::key_package_history::StoredKeyPackageHistoryEntry, StorageError>;

        fn find_key_package_history_entry_by_hash_ref(
            &self,
            hash_ref: Vec<u8>,
        ) -> Result<crate::key_package_history::StoredKeyPackageHistoryEntry, StorageError>;

        fn find_key_package_history_entries_before_id(
            &self,
            id: i32,
        ) -> Result<Vec<crate::key_package_history::StoredKeyPackageHistoryEntry>, StorageError>;

        fn mark_key_package_before_id_to_be_deleted(&self, id: i32) -> Result<(), StorageError>;

        fn get_expired_key_packages(
            &self,
        ) -> Result<Vec<crate::key_package_history::StoredKeyPackageHistoryEntry>, StorageError>;

        fn delete_key_package_history_up_to_id(&self, id: i32) -> Result<(), StorageError>;

        fn delete_key_package_entry_with_id(&self, id: i32) -> Result<(), StorageError>;
    }

    impl QueryKeyStoreEntry for DbQuery {
        fn insert_or_update_key_store_entry(
            &self,
            key: Vec<u8>,
            value: Vec<u8>,
        ) -> Result<(), StorageError>;
    }

    impl QueryDeviceSyncMessages for DbQuery {
        fn unprocessed_sync_group_messages(
            &self,
        ) -> Result<Vec<crate::group_message::StoredGroupMessage>, StorageError>;

        fn sync_group_messages_paged(
            &self,
            offset: i64,
            limit: i64,
        ) -> Result<Vec<crate::group_message::StoredGroupMessage>, StorageError>;

        fn mark_device_sync_msg_as_processed(
            &self,
            message_id: &[u8],
        ) -> Result<(), StorageError>;

        fn increment_device_sync_msg_attempt(
            &self,
            message_id: &[u8],
            max_attempts: i32,
        ) -> Result<i32, StorageError>;
    }

    impl QueryRefreshState for DbQuery {
        #[mockall::concretize]
        fn get_refresh_state<EntityId: AsRef<[u8]>>(
            &self,
            entity_id: EntityId,
            entity_kind: crate::refresh_state::EntityKind,
            originator_id: u32,
        ) -> Result<Option<crate::refresh_state::RefreshState>, StorageError>;

        #[mockall::concretize]
        fn get_last_cursor_for_originators<Id: AsRef<[u8]>>(
            &self,
            id: Id,
            entity_kind: crate::refresh_state::EntityKind,
            originator_id: &[u32]
        ) -> Result<Vec<Cursor>, StorageError>;

        #[mockall::concretize]
        fn get_last_cursor_for_ids<Id: AsRef<[u8]>>(
            &self,
            ids: &[Id],
            entities: &[crate::refresh_state::EntityKind],
        ) -> Result<std::collections::HashMap<Vec<u8>, GlobalCursor>, StorageError>;

        #[mockall::concretize]
        fn update_cursor<Id: AsRef<[u8]>>(
            &self,
            entity_id: Id,
            entity_kind: crate::refresh_state::EntityKind,
            cursor: xmtp_proto::types::Cursor
        ) -> Result<bool, StorageError>;

        #[mockall::concretize]
        fn get_remote_log_cursors(
            &self,
            conversation_ids: &[&Vec<u8>],
        ) -> Result<HashMap<Vec<u8>, Cursor>, crate::ConnectionError>;

        #[mockall::concretize]
        fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, StorageError>;

        #[mockall::concretize]
        fn latest_cursor_for_id<Id: AsRef<[u8]>>(
            &self,
            entity: Id,
            entities: &[crate::refresh_state::EntityKind],
            originators: Option<&[&xmtp_proto::types::OriginatorId]>
        ) -> Result<xmtp_proto::types::GlobalCursor, StorageError>;

        #[mockall::concretize]
        fn latest_cursor_combined<Id: AsRef<[u8]>>(
            &self,
            entity_id: Id,
            entities: &[crate::refresh_state::EntityKind],
            originators: Option<&[&xmtp_proto::types::OriginatorId]>,
        ) -> Result<GlobalCursor, StorageError>;

        #[mockall::concretize]
        fn lowest_common_cursor_combined(&self, topics: &[&Topic]) -> Result<GlobalCursor, StorageError>;
    }

    impl QueryIdentityUpdates for DbQuery {
        #[mockall::concretize]
        fn get_identity_updates<InboxId: AsRef<str>>(
            &self,
            inbox_id: InboxId,
            from_sequence_id: Option<i64>,
            to_sequence_id: Option<i64>,
        ) -> Result<Vec<crate::identity_update::StoredIdentityUpdate>, crate::ConnectionError>;

        fn insert_or_ignore_identity_updates(
            &self,
            updates: &[crate::identity_update::StoredIdentityUpdate],
        ) -> Result<(), crate::ConnectionError>;

        fn get_latest_sequence_id_for_inbox(
            &self,
            inbox_id: &str,
        ) -> Result<i64, crate::ConnectionError>;

        fn get_latest_sequence_id<'a>(
            &'a self,
            inbox_ids: &'a [&'a str],
        ) -> Result<std::collections::HashMap<String, i64>, crate::ConnectionError>;

        fn count_inbox_updates<'a>(
            &'a self,
            inbox_ids: &'a [&'a str],
        ) -> Result<std::collections::HashMap<String, i64>, crate::ConnectionError>;
    }

    impl QueryLocalCommitLog for DbQuery {
        fn get_group_logs(
            &self,
            group_id: &[u8],
        ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError>;

        // Local commit log entries are returned sorted in ascending order of `rowid`
        // Entries with `commit_sequence_id` = 0 should not be published to the remote commit log
        fn get_local_commit_log_after_cursor(
            &self,
            group_id: &[u8],
            after_cursor: i64,
            order_by: LocalCommitLogOrder,
        ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError>;

        fn get_latest_log_for_group(
            &self,
            group_id: &[u8],
        ) -> Result<Option<LocalCommitLog>, crate::ConnectionError>;

        fn get_local_commit_log_cursor(
            &self,
            group_id: &[u8],
        ) -> Result<Option<i32>, crate::ConnectionError>;
    }

    impl QueryRemoteCommitLog for DbQuery {
        fn get_latest_remote_log_for_group(&self, group_id: &[u8]) -> Result<Option<RemoteCommitLog>, crate::ConnectionError>;

        fn get_remote_commit_log_after_cursor(
            &self,
            group_id: &[u8],
            after_cursor: i64,
            order_by: RemoteCommitLogOrder,
        ) -> Result<Vec<RemoteCommitLog>, crate::ConnectionError>;

    }

    impl QueryAssociationStateCache for DbQuery {
        fn write_to_cache(
            &self,
            inbox_id: String,
            sequence_id: i64,
            state: AssociationStateProto,
        ) -> Result<(), StorageError>;

        #[mockall::concretize]
        fn read_from_cache<A: AsRef<str>>(
            &self,
            inbox_id: A,
            sequence_id: i64,
        ) -> Result<Option<AssociationStateProto>, StorageError>;


        #[mockall::concretize]
        fn batch_read_from_cache(
            &self,
            identifiers: Vec<(String, i64)>,
        ) -> Result<Vec<AssociationStateProto>, StorageError>;
    }

    impl QueryTasks for DbQuery {
        fn create_task(&self, task: crate::tasks::NewTask) -> Result<crate::tasks::Task, StorageError>;

        fn get_tasks(&self) -> Result<Vec<crate::tasks::Task>, StorageError>;

        fn get_next_task(&self) -> Result<Option<crate::tasks::Task>, StorageError>;

        fn update_task(
            &self,
            id: i32,
            attempts: i32,
            last_attempted_at_ns: i64,
            next_attempt_at_ns: i64,
        ) -> Result<crate::tasks::Task, StorageError>;

        fn delete_task(&self, id: i32) -> Result<bool, StorageError>;
    }

    impl Pragmas for DbQuery {
        fn busy_timeout(
            &self,
        ) -> Result<i32, crate::ConnectionError>;
        #[mockall::concretize]
        fn set_sqlcipher_log<S: AsRef<str>>(
            &self,
            level: S
        ) -> Result<(), crate::ConnectionError>;
    }

    impl QueryPendingRemove for DbQuery{
        fn get_pending_remove_users(
        &self,
        group_id: &[u8],
    ) -> Result<Vec<String>, crate::ConnectionError>;
        fn delete_pending_remove_users(
        &self,
            group_id: &[u8],
            inbox_ids: Vec<String>,
        ) -> Result<usize, crate::ConnectionError>;
             fn get_user_pending_remove_status(&self,
            group_id: &[u8],
            inbox_id: &str,
        ) -> Result<bool, crate::ConnectionError>;
    }

    impl QueryIcebox for DbQuery {
        fn past_dependents(
            &self,
            cursors: &[xmtp_proto::types::Cursor],
        ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError>;

        fn future_dependents(
            &self,
            cursors: &[xmtp_proto::types::Cursor],
        ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError>;

        fn ice(
            &self,
            orphans: Vec<OrphanedEnvelope>,
        ) -> Result<usize, crate::ConnectionError>;

        fn prune_icebox(&self) -> Result<usize, crate::ConnectionError>;
    }

    impl crate::migrations::QueryMigrations for DbQuery {
        fn applied_migrations(&self) -> Result<Vec<String>, crate::ConnectionError>;

        fn available_migrations(&self) -> Result<Vec<String>, crate::ConnectionError>;

        fn rollback_to_version<'a>(
            &self,
            version: &'a str,
        ) -> Result<Vec<String>, crate::ConnectionError>;

        fn run_migration<'a>(
            &self,
            name: &'a str,
        ) -> Result<(), crate::ConnectionError>;

        fn revert_migration<'a>(
            &self,
            name: &'a str,
        ) -> Result<(), crate::ConnectionError>;

        fn run_pending_migrations(&self) -> Result<Vec<String>, crate::ConnectionError>;
    }
    impl crate::d14n_migration_cutover::QueryMigrationCutover for DbQuery {
        fn get_migration_cutover(&self) -> Result<crate::d14n_migration_cutover::StoredMigrationCutover, StorageError>;

        fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), StorageError>;

        fn get_last_checked_ns(&self) -> Result<i64, StorageError>;

        fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), StorageError>;

        fn set_has_migrated(&self, has_migrated: bool) -> Result<(), StorageError>;
    }

    impl crate::message_deletion::QueryMessageDeletion for DbQuery {
        fn get_message_deletion(
            &self,
            _id: &[u8],
        ) -> Result<Option<crate::message_deletion::StoredMessageDeletion>, crate::ConnectionError>;

        fn get_deletion_by_deleted_message_id(
            &self,
            _deleted_message_id: &[u8],
        ) -> Result<Option<crate::message_deletion::StoredMessageDeletion>, crate::ConnectionError>;

        fn get_deletions_for_messages(
            &self,
            _message_ids: Vec<Vec<u8>>,
        ) -> Result<Vec<crate::message_deletion::StoredMessageDeletion>, crate::ConnectionError>;

        fn get_group_deletions(
            &self,
            _group_id: &[u8],
        ) -> Result<Vec<crate::message_deletion::StoredMessageDeletion>, crate::ConnectionError>;

        fn is_message_deleted(
            &self,
            _message_id: &[u8],
        ) -> Result<bool, crate::ConnectionError>;
    }

}

impl ConnectionExt for MockDbQuery {
    fn raw_query_read<T, F>(&self, _fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        todo!()
    }

    fn raw_query_write<T, F>(&self, _fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        // usually OK because we seldom use the result of a write
        tracing::warn!("unhandled mock raw_query_write");
        unsafe {
            let uninit = std::mem::MaybeUninit::<T>::uninit();
            Ok(uninit.assume_init())
        }
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        todo!()
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        todo!()
    }
}

impl IntoConnection for MockDbQuery {
    type Connection = MockConnection;

    fn into_connection(self) -> Self::Connection {
        todo!()
    }
}
