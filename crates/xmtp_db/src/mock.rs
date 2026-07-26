use crate::StorageError;
use crate::association_state::QueryAssociationStateCache;
use crate::group::ConversationType;
use crate::group::StoredGroupCommitLogPublicKey;
use crate::group_message::StoredGroupMessage;
use crate::local_commit_log::{LocalCommitLog, LocalCommitLogOrder};
use crate::remote_commit_log::{RemoteCommitLog, RemoteCommitLogOrder};
use std::collections::HashMap;
use std::sync::Arc;
use xmtp_proto::types::{Cursor, GlobalCursor, GroupId, OrphanedEnvelope};
use xmtp_proto::xmtp::identity::associations::AssociationState as AssociationStateProto;

use crate::SqliteConnection;
use crate::prelude::*;
use mockall::mock;
use parking_lot::Mutex;

use crate::pending_remove::QueryPendingRemove;
#[cfg(feature = "sync")]
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
#[cfg(feature = "sync")]
impl ConnectionExt for MockConnection {
    fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
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
        async fn get_consent_record(
            &self,
            entity: String,
            entity_type: crate::consent_record::ConsentType,
        ) -> Result<Option<crate::consent_record::StoredConsentRecord>, crate::ConnectionError>;

        async fn consent_records(
            &self,
        ) -> Result<Vec<crate::consent_record::StoredConsentRecord>, crate::ConnectionError>;

        async fn consent_records_paged(
            &self,
            limit: i64,
            offset: i64,
        ) -> Result<Vec<crate::consent_record::StoredConsentRecord>, crate::ConnectionError>;

        async fn insert_newer_consent_record(
            &self,
            record: crate::consent_record::StoredConsentRecord,
        ) -> Result<bool, crate::ConnectionError>;

        async fn insert_or_replace_consent_records(
            &self,
            records: &[crate::consent_record::StoredConsentRecord],
        ) -> Result<Vec<crate::consent_record::StoredConsentRecord>, crate::ConnectionError>;

        async fn maybe_insert_consent_record_return_existing(
            &self,
            record: &crate::consent_record::StoredConsentRecord,
        ) -> Result<Option<crate::consent_record::StoredConsentRecord>, crate::ConnectionError>;

        async fn find_consent_by_dm_id(
            &self,
            dm_id: &str,
        ) -> Result<Vec<crate::consent_record::StoredConsentRecord>, crate::ConnectionError>;
    }

    impl QueryConversationList for DbQuery {
        async fn fetch_conversation_list(
            &self,
            args: &crate::group::GroupQueryArgs,
        ) -> Result<Vec<crate::conversation_list::ConversationListItem>, StorageError>;
    }

    impl QueryDms for DbQuery {
        async fn fetch_stitched(
            &self,
            key: &GroupId,
        ) -> Result<Option<crate::group::StoredGroup>, ConnectionError>;

        async fn find_active_dm_group(
            &self,
            members: &str,
        ) -> Result<Option<crate::group::StoredGroup>, ConnectionError>;

        async fn other_dms(&self, group_id: &GroupId)
        -> Result<Vec<crate::group::StoredGroup>, ConnectionError>;
    }

    impl QueryGroup for DbQuery {
        async fn find_groups(
            &self,
            args: &crate::group::GroupQueryArgs,
        ) -> Result<Vec<crate::group::StoredGroup>, crate::ConnectionError>;

        async fn find_groups_by_id_paged(
            &self,
            args: &crate::group::GroupQueryArgs,
            offset: i64,
        ) -> Result<Vec<crate::group::StoredGroup>, crate::ConnectionError>;

        async fn update_group_membership(
            &self,
            group_id: &GroupId,
            state: crate::group::GroupMembershipState,
        ) -> Result<(), crate::ConnectionError>;

        async fn all_sync_groups(&self) -> Result<Vec<crate::group::StoredGroup>, crate::ConnectionError>;

        async fn find_sync_group(
            &self,
            id: &GroupId,
        ) -> Result<Option<crate::group::StoredGroup>, crate::ConnectionError>;

        async fn primary_sync_group(
            &self,
        ) -> Result<Option<crate::group::StoredGroup>, crate::ConnectionError>;

        async fn find_group(
            &self,
            id: &GroupId,
        ) -> Result<Option<crate::group::StoredGroup>, crate::ConnectionError>;

        async fn find_group_by_sequence_id(
            &self,
            cursor: Cursor,
        ) -> Result<Option<crate::group::StoredGroup>, crate::ConnectionError>;

        async fn get_rotated_at_ns(&self, group_id: &GroupId) -> Result<i64, StorageError>;

        async fn update_rotated_at_ns(&self, group_id: &GroupId) -> Result<(), StorageError>;

        async fn get_installations_time_checked(&self, group_id: &GroupId) -> Result<i64, StorageError>;

        async fn update_installations_time_checked(&self, group_id: &GroupId) -> Result<(), StorageError>;

        async fn update_message_disappearing_from_ns(
            &self,
            group_id: &GroupId,
            from_ns: Option<i64>,
        ) -> Result<(), StorageError>;

        async fn update_message_disappearing_in_ns(
            &self,
            group_id: &GroupId,
            in_ns: Option<i64>,
        ) -> Result<(), StorageError>;

        async fn insert_or_replace_group(
            &self,
            group: crate::group::StoredGroup,
        ) -> Result<crate::group::StoredGroup, StorageError>;

        async fn group_cursors(&self) -> Result<Vec<Cursor>, crate::ConnectionError>;

        async fn mark_group_as_maybe_forked(
            &self,
            group_id: &GroupId,
            fork_details: String,
        ) -> Result<(), StorageError>;

        async fn clear_fork_flag_for_group(&self, group_id: &GroupId) -> Result<(), crate::ConnectionError>;

        async fn has_duplicate_dm(&self, group_id: &GroupId) -> Result<bool, crate::ConnectionError>;

        async fn get_conversation_ids_for_remote_log_publish(&self) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError>;

        async fn get_conversation_ids_for_remote_log_download(&self) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError>;

        async fn get_conversation_ids_for_fork_check(
            &self,
        ) -> Result<Vec<Vec<u8>>, crate::ConnectionError>;

        async fn get_conversation_ids_for_requesting_readds(
            &self,
        ) -> Result<Vec<crate::encrypted_store::group::StoredGroupForReaddRequest>, crate::ConnectionError>;

        async fn get_conversation_ids_for_responding_readds(
            &self,
        ) -> Result<Vec<crate::encrypted_store::group::StoredGroupForRespondingReadds>, crate::ConnectionError>;

        async fn get_conversation_type(&self, group_id: &GroupId) -> Result<ConversationType, crate::ConnectionError>;

        async fn set_group_commit_log_public_key(
            &self,
            group_id: &GroupId,
            public_key: &[u8],
        ) -> Result<(), StorageError>;

        async fn set_group_commit_log_forked_status(
            &self,
            group_id: &GroupId,
            is_forked: Option<bool>,
        ) -> Result<(), StorageError>;

        async fn get_group_commit_log_forked_status(
            &self,
            group_id: &GroupId,
        ) -> Result<Option<bool>, StorageError>;

        async fn set_group_has_pending_leave_request_status(
            &self,
            group_id: &GroupId,
            has_pending_leave_request: Option<bool>,
        ) -> Result<(), StorageError>;
            async fn get_groups_have_pending_leave_request(
        &self,
    ) -> Result<Vec<Vec<u8>>, crate::ConnectionError>;
    }

    impl QueryGroupVersion for DbQuery {
        async fn set_group_paused(&self, group_id: &GroupId, min_version: &str) -> Result<(), StorageError>;

        async fn unpause_group(&self, group_id: &GroupId) -> Result<(), StorageError>;

        async fn get_group_paused_version(&self, group_id: &GroupId) -> Result<Option<String>, StorageError>;

        async fn get_paused_groups_with_versions(&self) -> Result<Vec<(GroupId, String)>, StorageError>;
    }

    impl QueryGroupIntent for DbQuery {
        async fn insert_group_intent(
            &self,
            to_save: crate::group_intent::NewGroupIntent,
        ) -> Result<crate::group_intent::StoredGroupIntent, crate::ConnectionError>;

        async fn find_group_intents(
            &self,
            group_id: &GroupId,
            allowed_states: Option<Vec<crate::group_intent::IntentState>>,
            allowed_kinds: Option<Vec<crate::group_intent::IntentKind>>,
        ) -> Result<Vec<crate::group_intent::StoredGroupIntent>, crate::ConnectionError>;

        async fn set_group_intent_published(
            &self,
            intent_id: crate::group_intent::ID,
            payload_hash: &[u8],
            post_commit_data: Option<Vec<u8>>,
            staged_commit: Option<Vec<u8>>,
            published_in_epoch: i64,
        ) -> Result<(), StorageError>;

        async fn set_group_intent_committed(
            &self,
            intent_id: crate::group_intent::ID,
            cursor: Cursor,
        ) -> Result<(), StorageError>;

        async fn set_group_intent_processed(
            &self,
            intent_id: crate::group_intent::ID,
        ) -> Result<(), StorageError>;

        async fn set_group_intent_superseded(
            &self,
            intent_id: crate::group_intent::ID,
        ) -> Result<(), StorageError>;

        async fn set_group_intent_to_publish(
            &self,
            intent_id: crate::group_intent::ID,
        ) -> Result<(), StorageError>;

        async fn set_group_intent_error(
            &self,
            intent_id: crate::group_intent::ID,
        ) -> Result<(), StorageError>;

        async fn find_group_intent_by_payload_hash(
            &self,
            payload_hash: &[u8],
        ) -> Result<Option<crate::group_intent::StoredGroupIntent>, StorageError>;

        async fn find_dependant_commits<'a>(
            &self,
            payload_hashes: &[&'a [u8]],
        ) -> Result<HashMap<crate::group_intent::PayloadHash, crate::group_intent::IntentDependency>, StorageError>;

        async fn increment_intent_publish_attempt_count(
            &self,
            intent_id: crate::group_intent::ID,
        ) -> Result<(), StorageError>;

        async fn set_group_intent_error_and_fail_msg(
            &self,
            intent: &crate::group_intent::StoredGroupIntent,
            msg_id: Option<Vec<u8>>,
        ) -> Result<(), StorageError>;
    }

    impl QueryReaddStatus for DbQuery {
        async fn get_readd_status(
            &self,
            group_id: &GroupId,
            installation_id: &[u8],
        ) -> Result<Option<crate::readd_status::ReaddStatus>, crate::ConnectionError>;

        async fn is_awaiting_readd(
            &self,
            group_id: &GroupId,
            installation_id: &[u8],
        ) -> Result<bool, crate::ConnectionError>;

        async fn update_requested_at_sequence_id(
            &self,
            group_id: &GroupId,
            installation_id: &[u8],
            sequence_id: i64,
        ) -> Result<(), crate::ConnectionError>;

        async fn update_responded_at_sequence_id(
            &self,
            group_id: &GroupId,
            installation_id: &[u8],
            sequence_id: i64,
        ) -> Result<(), crate::ConnectionError>;

        async fn delete_other_readd_statuses(
            &self,
            group_id: &GroupId,
            self_installation_id: &[u8],
        ) -> Result<(), crate::ConnectionError>;

        async fn delete_readd_statuses(
            &self,
            group_id: &GroupId,
            installation_ids: std::collections::HashSet<Vec<u8> > ,
        ) -> Result<(), crate::ConnectionError>;

        async fn get_readds_awaiting_response(
            &self,
            group_id: &GroupId,
            self_installation_id: &[u8],
        ) -> Result<Vec<crate::readd_status::ReaddStatus>, crate::ConnectionError>;
    }

    impl QueryGroupMessage for DbQuery {
        async fn get_group_messages(
            &self,
            group_id: &GroupId,
            args: &crate::group_message::MsgQueryArgs,
        ) -> Result<Vec<crate::group_message::StoredGroupMessage>, crate::ConnectionError>;

        async fn count_group_messages(
            &self,
            group_id: &GroupId,
            args: &crate::group_message::MsgQueryArgs,
        ) -> Result<i64, crate::ConnectionError>;

        async fn missing_messages(
            &self,
            group_id: &GroupId,
            sequence_ids: &[u64],
        ) -> Result<Vec<crate::group_message::StoredGroupMessage>, crate::ConnectionError>;

        async fn group_messages_paged(
            &self,
            args: &crate::group_message::MsgQueryArgs,
            offset: i64,
        ) -> Result<Vec<crate::group_message::StoredGroupMessage>, crate::ConnectionError>;

        async fn get_group_messages_with_reactions(
            &self,
            group_id: &GroupId,
            args: &crate::group_message::MsgQueryArgs,
        ) -> Result<Vec<crate::group_message::StoredGroupMessageWithReactions>, crate::ConnectionError>;

        async fn get_inbound_relations<'a>(
            &self,
            group_id: &GroupId,
            message_ids: &[&'a [u8]],
            relation_query: crate::group_message::RelationQuery,
        ) -> Result<crate::group_message::InboundRelations, crate::ConnectionError>;

        async fn get_outbound_relations<'a>(
            &self,
            group_id: &GroupId,
            message_ids: &[&'a [u8]],
        ) -> Result<crate::group_message::OutboundRelations, crate::ConnectionError>;

        async fn get_inbound_relation_counts<'a>(
            &self,
            group_id: &GroupId,
            message_ids: &[&'a [u8]],
            relation_query: crate::group_message::RelationQuery,
        ) -> Result<crate::group_message::RelationCounts, crate::ConnectionError>;

        async fn get_group_message(
            &self,
            id: &[u8],
        ) -> Result<Option<crate::group_message::StoredGroupMessage>, crate::ConnectionError>;

        async fn write_conn_get_group_message(
            &self,
            id: &[u8],
        ) -> Result<Option<crate::group_message::StoredGroupMessage>, crate::ConnectionError>;

        async fn get_group_message_by_timestamp(
            &self,
            group_id: &GroupId,
            timestamp: i64,
        ) -> Result<Option<crate::group_message::StoredGroupMessage>, crate::ConnectionError>;

        async fn get_group_message_by_cursor(
            &self,
            group_id: &GroupId,
            sequence_id: Cursor,
        ) -> Result<Option<crate::group_message::StoredGroupMessage>, crate::ConnectionError>;

        async fn set_delivery_status_to_published(
            &self,
            msg_id: &[u8],
            timestamp: u64,
            cursor: Cursor,
            message_expire_at_ns: Option<i64>
        ) -> Result<usize, crate::ConnectionError>;

        async fn set_delivery_status_to_failed(
            &self,
            msg_id: &[u8],
        ) -> Result<usize, crate::ConnectionError>;

        async fn delete_expired_messages(&self) -> Result<Vec<StoredGroupMessage>, crate::ConnectionError>;

        async fn min_expire_at_ns(&self) -> Result<Option<i64>, crate::ConnectionError>;

        async fn delete_message_by_id(
            &self,
            message_id: &[u8],
        ) -> Result<usize, crate::ConnectionError>;

        async fn get_latest_message_times_by_sender(
            &self,
            group_id: &GroupId,
            allowed_content_types: &[crate::group_message::ContentType],
        ) -> Result<crate::group_message::LatestMessageTimeBySender, crate::ConnectionError>;

        async fn messages_newer_than(
            &self,
            cursors_by_group: &HashMap<Vec<u8>, xmtp_proto::types::GlobalCursor>,
        ) -> Result<Vec<(GroupId, Cursor)>, crate::ConnectionError>;

        async fn clear_messages<'a>(
            &self,
            group_ids: Option<&'a [GroupId]>,
            retention_days: Option<u32>,
        ) -> Result<usize, crate::ConnectionError>;
    }

    impl QueryIdentity for DbQuery {
        async fn queue_key_package_rotation(&self) -> Result<(), StorageError>;
        async fn queue_key_rotation_with_nudge(&self, rotation_task_hash: &crate::tasks::TaskDataHash, rotation_seed: crate::tasks::NewTask) -> Result<(), StorageError>;

        async fn reset_key_package_rotation_queue(&self, rotation_interval: i64) -> Result<(), StorageError>;

        async fn is_identity_needs_rotation(&self) -> Result<bool, StorageError>;

        async fn next_key_package_rotation_ns(&self) -> Result<Option<i64>, StorageError>;
    }

    impl QueryIdentityCache for DbQuery {
        async fn fetch_cached_inbox_ids(
            &self,
            identifiers: &[(String, crate::identity_cache::StoredIdentityKind)],
        ) -> Result<HashMap<String, String>, StorageError>;

        async fn cache_inbox_id(
            &self,
            kind: crate::identity_cache::StoredIdentityKind,
            identity: String,
            inbox_id: &str,
        ) -> Result<(), StorageError>;
    }

    impl QueryKeyPackageHistory for DbQuery {
        async fn store_key_package_history_entry(
            &self,
            key_package_hash_ref: Vec<u8>,
            post_quantum_public_key: Option<Vec<u8>>,
        ) -> Result<crate::key_package_history::StoredKeyPackageHistoryEntry, StorageError>;

        async fn find_key_package_history_entry_by_hash_ref(
            &self,
            hash_ref: Vec<u8>,
        ) -> Result<crate::key_package_history::StoredKeyPackageHistoryEntry, StorageError>;

        async fn find_key_package_history_entries_before_id(
            &self,
            id: i32,
        ) -> Result<Vec<crate::key_package_history::StoredKeyPackageHistoryEntry>, StorageError>;

        async fn mark_key_package_before_id_to_be_deleted(&self, id: i32) -> Result<(), StorageError>;

        async fn get_expired_key_packages(
            &self,
        ) -> Result<Vec<crate::key_package_history::StoredKeyPackageHistoryEntry>, StorageError>;

        async fn min_key_package_delete_at_ns(&self) -> Result<Option<i64>, StorageError>;

        async fn delete_key_package_history_up_to_id(&self, id: i32) -> Result<(), StorageError>;

        async fn delete_key_package_entry_with_id(&self, id: i32) -> Result<(), StorageError>;
    }

    impl QueryKeyStoreEntry for DbQuery {
        async fn insert_or_update_key_store_entry(
            &self,
            key: Vec<u8>,
            value: Vec<u8>,
        ) -> Result<(), StorageError>;
    }

    impl QueryDeviceSyncMessages for DbQuery {
        async fn unprocessed_sync_group_messages(
            &self,
        ) -> Result<Vec<crate::group_message::StoredGroupMessage>, StorageError>;

        async fn sync_group_messages_paged(
            &self,
            offset: i64,
            limit: i64,
        ) -> Result<Vec<crate::group_message::StoredGroupMessage>, StorageError>;

        async fn mark_device_sync_msg_as_processed(
            &self,
            message_id: &[u8],
        ) -> Result<(), StorageError>;

        async fn increment_device_sync_msg_attempt(
            &self,
            message_id: &[u8],
            max_attempts: i32,
        ) -> Result<i32, StorageError>;
    }

    impl QueryRefreshState for DbQuery {
        async fn get_refresh_state(
            &self,
            entity_id: &[u8],
            entity_kind: crate::refresh_state::EntityKind,
            originator_id: u32,
        ) -> Result<Option<crate::refresh_state::RefreshState>, StorageError>;

        async fn get_last_cursor_for_originators(
            &self,
            id: &[u8],
            entity_kind: crate::refresh_state::EntityKind,
            originator_id: &[u32]
        ) -> Result<Vec<Cursor>, StorageError>;

        // The one `Query*` method that is still generic (see `QueryRefreshState`),
        // so it is also the one that still needs `concretize`.
        #[mockall::concretize]
        async fn get_last_cursor_for_ids<Id: crate::refresh_state::EntityIdBytes>(
            &self,
            ids: &[Id],
            entities: &[crate::refresh_state::EntityKind],
        ) -> Result<std::collections::HashMap<Vec<u8>, GlobalCursor>, StorageError>;

        async fn update_cursor(
            &self,
            entity_id: &[u8],
            entity_kind: crate::refresh_state::EntityKind,
            cursor: xmtp_proto::types::Cursor
        ) -> Result<bool, StorageError>;

        async fn get_remote_log_cursors<'a>(
            &self,
            conversation_ids: &[&'a [u8]],
        ) -> Result<HashMap<Vec<u8>, Cursor>, crate::ConnectionError>;

        async fn latest_cursor_for_id<'a, 'b>(
            &self,
            entity: &[u8],
            entities: &[crate::refresh_state::EntityKind],
            originators: Option<&'a [&'b xmtp_proto::types::OriginatorId]>
        ) -> Result<xmtp_proto::types::GlobalCursor, StorageError>;

    }

    impl QueryIdentityUpdates for DbQuery {
        async fn get_identity_updates(
            &self,
            inbox_id: &str,
            from_sequence_id: Option<i64>,
            to_sequence_id: Option<i64>,
        ) -> Result<Vec<crate::identity_update::StoredIdentityUpdate>, crate::ConnectionError>;

        async fn insert_or_ignore_identity_updates(
            &self,
            updates: &[crate::identity_update::StoredIdentityUpdate],
        ) -> Result<(), crate::ConnectionError>;

        async fn get_latest_sequence_id_for_inbox(
            &self,
            inbox_id: &str,
        ) -> Result<i64, crate::ConnectionError>;

        async fn get_latest_sequence_id<'a>(
            &self,
            inbox_ids: &[&'a str],
        ) -> Result<std::collections::HashMap<String, i64>, crate::ConnectionError>;

        async fn count_inbox_updates<'a>(
            &self,
            inbox_ids: &[&'a str],
        ) -> Result<std::collections::HashMap<String, i64>, crate::ConnectionError>;
    }

    impl QueryLocalCommitLog for DbQuery {
        async fn get_group_logs(
            &self,
            group_id: &GroupId,
        ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError>;

        // Local commit log entries are returned sorted in ascending order of `rowid`
        // Entries with `commit_sequence_id` = 0 should not be published to the remote commit log
        async fn get_local_commit_log_after_cursor(
            &self,
            group_id: &GroupId,
            after_cursor: i64,
            order_by: LocalCommitLogOrder,
        ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError>;

        async fn get_latest_log_for_group(
            &self,
            group_id: &GroupId,
        ) -> Result<Option<LocalCommitLog>, crate::ConnectionError>;

        async fn get_local_commit_log_cursor(
            &self,
            group_id: &GroupId,
        ) -> Result<Option<i32>, crate::ConnectionError>;

        async fn get_latest_chain_start_rowid(
            &self,
            group_id: &GroupId,
        ) -> Result<Option<i32>, crate::ConnectionError>;
    }

    impl QueryRemoteCommitLog for DbQuery {
        async fn get_latest_remote_log_for_group(&self, group_id: &GroupId) -> Result<Option<RemoteCommitLog>, crate::ConnectionError>;

        async fn get_remote_commit_log_after_cursor(
            &self,
            group_id: &GroupId,
            after_cursor: i64,
            order_by: RemoteCommitLogOrder,
        ) -> Result<Vec<RemoteCommitLog>, crate::ConnectionError>;

    }

    impl QueryAssociationStateCache for DbQuery {
        async fn write_to_cache(
            &self,
            inbox_id: String,
            sequence_id: i64,
            state: AssociationStateProto,
        ) -> Result<(), StorageError>;

        async fn read_from_cache(
            &self,
            inbox_id: &str,
            sequence_id: i64,
        ) -> Result<Option<AssociationStateProto>, StorageError>;


        async fn batch_read_from_cache(
            &self,
            identifiers: Vec<(String, i64)>,
        ) -> Result<Vec<AssociationStateProto>, StorageError>;
    }

    impl QueryTasks for DbQuery {
        async fn create_task(&self, task: crate::tasks::NewTask) -> Result<crate::tasks::Task, StorageError>;

        async fn create_or_ignore_task(&self, task: crate::tasks::NewTask) -> Result<(), StorageError>;

        async fn pull_in_task_deadline(&self, target_data_hash: &crate::tasks::TaskDataHash, at_ns: i64) -> Result<bool, StorageError>;

        async fn get_tasks(&self) -> Result<Vec<crate::tasks::Task>, StorageError>;

        async fn get_next_task(&self) -> Result<Option<crate::tasks::Task>, StorageError>;

        async fn upsert_pending_self_remove_task(&self, group_id: &GroupId, task: crate::tasks::NewTask) -> Result<(), StorageError>;

        async fn update_task(
            &self,
            id: i32,
            attempts: i32,
            last_attempted_at_ns: i64,
            next_attempt_at_ns: i64,
        ) -> Result<crate::tasks::Task, StorageError>;

        async fn delete_task(&self, id: i32) -> Result<bool, StorageError>;
    }

    impl Pragmas for DbQuery {
        fn busy_timeout(
            &self,
        ) -> Result<i32, crate::ConnectionError>;
        fn set_sqlcipher_log(
            &self,
            level: &str
        ) -> Result<(), crate::ConnectionError>;
    }

    impl QueryPendingRemove for DbQuery{
        async fn get_pending_remove_users(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<String>, crate::ConnectionError>;
        async fn delete_pending_remove_users(
        &self,
            group_id: &GroupId,
            inbox_ids: Vec<String>,
        ) -> Result<usize, crate::ConnectionError>;
             async fn get_user_pending_remove_status(&self,
            group_id: &GroupId,
            inbox_id: &str,
        ) -> Result<bool, crate::ConnectionError>;
    }

    impl QueryIcebox for DbQuery {
        async fn past_dependents(
            &self,
            cursors: &[xmtp_proto::types::Cursor],
        ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError>;

        async fn future_dependents(
            &self,
            cursors: &[xmtp_proto::types::Cursor],
        ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError>;

        async fn ice(
            &self,
            orphans: Vec<OrphanedEnvelope>,
        ) -> Result<usize, crate::ConnectionError>;

        async fn prune_icebox(&self) -> Result<usize, crate::ConnectionError>;
    }

    impl crate::migrations::QueryMigrations for DbQuery {
        async fn applied_migrations(&self) -> Result<Vec<String>, crate::ConnectionError>;

        async fn available_migrations(&self) -> Result<Vec<String>, crate::ConnectionError>;

        async fn rollback_to_version(
            &self,
            version: &str,
        ) -> Result<Vec<String>, crate::ConnectionError>;

        async fn run_migration(
            &self,
            name: &str,
        ) -> Result<(), crate::ConnectionError>;

        async fn revert_migration(
            &self,
            name: &str,
        ) -> Result<(), crate::ConnectionError>;

        async fn run_pending_migrations(&self) -> Result<Vec<String>, crate::ConnectionError>;
    }
    impl crate::user_preferences::QueryUserPreferences for DbQuery {
        async fn load_user_preferences(
            &self,
        ) -> Result<crate::user_preferences::StoredUserPreferences, StorageError>;

        async fn store_hmac_key(
            &self,
            key: &[u8],
            cycled_at_ns: Option<i64>,
        ) -> Result<(), StorageError>;

        async fn set_dm_group_updates_migrated(&self) -> Result<(), StorageError>;
    }

    impl crate::d14n_migration_cutover::QueryMigrationCutover for DbQuery {
        async fn get_migration_cutover(&self) -> Result<crate::d14n_migration_cutover::StoredMigrationCutover, StorageError>;

        async fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), StorageError>;

        async fn get_last_checked_ns(&self) -> Result<i64, StorageError>;

        async fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), StorageError>;

        async fn set_has_migrated(&self, has_migrated: bool) -> Result<(), StorageError>;
    }

    impl crate::message_deletion::QueryMessageDeletion for DbQuery {
        async fn get_message_deletion(
            &self,
            _id: &[u8],
        ) -> Result<Option<crate::message_deletion::StoredMessageDeletion>, crate::ConnectionError>;

        async fn get_deletion_by_deleted_message_id(
            &self,
            _deleted_message_id: &[u8],
        ) -> Result<Option<crate::message_deletion::StoredMessageDeletion>, crate::ConnectionError>;

        async fn get_deletions_for_messages(
            &self,
            _message_ids: Vec<Vec<u8>>,
        ) -> Result<Vec<crate::message_deletion::StoredMessageDeletion>, crate::ConnectionError>;

        async fn get_group_deletions(
            &self,
            _group_id: &GroupId,
        ) -> Result<Vec<crate::message_deletion::StoredMessageDeletion>, crate::ConnectionError>;

        async fn is_message_deleted(
            &self,
            _message_id: &[u8],
        ) -> Result<bool, crate::ConnectionError>;
    }

}

#[cfg(feature = "sync")]
impl ConnectionExt for MockDbQuery {
    fn raw_query<T, F>(&self, _fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        // usually OK because we seldom use the result
        tracing::warn!("unhandled mock raw_query");
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
