use crate::groups::commit_log_key::CommitLogKeyCrypto;
use crate::groups::commit_log_key::derive_consensus_public_key;
use futures::StreamExt;
use openmls::prelude::OpenMlsCrypto;
use openmls::prelude::SignatureScheme;
use openmls_traits::OpenMlsProvider;
use prost::Message;
use std::{collections::HashMap, time::Duration};
use thiserror::Error;
use xmtp_api::ApiError;
use xmtp_db::remote_commit_log::RemoteCommitLog;
use xmtp_db::{
    DbQuery, StorageError, Store,
    group::StoredGroupCommitLogPublicKey,
    local_commit_log::LocalCommitLogOrder,
    prelude::*,
    remote_commit_log::{self, CommitResult, NewRemoteCommitLog},
};
use xmtp_proto::mls_v1::PublishCommitLogRequest;
use xmtp_proto::xmtp::identity::associations::RecoverableEd25519Signature;
use xmtp_proto::xmtp::mls::message_contents::{CommitLogEntry, CommitResult as ProtoCommitResult};
use xmtp_proto::{
    mls_v1::{PagingInfo, QueryCommitLogRequest, QueryCommitLogResponse},
    xmtp::{message_api::v1::SortDirection, mls::message_contents::PlaintextCommitLogEntry},
};

use crate::groups::commit_log_key::get_or_create_signing_key;
use crate::{
    context::XmtpSharedContext,
    groups::GroupError,
    worker::{BoxedWorker, NeedsDbReconnect, Worker, WorkerFactory, WorkerKind, WorkerResult},
};

/// Interval at which the CommitLogWorker runs to publish commit log entries.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(60 * 5); // 5 minutes

#[derive(Clone)]
pub struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + Send + Sync + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::CommitLog
    }

    fn create(
        &self,
        metrics: Option<crate::worker::DynMetrics>,
    ) -> (BoxedWorker, Option<crate::worker::DynMetrics>) {
        (
            Box::new(CommitLogWorker::new(self.context.clone())) as Box<_>,
            metrics,
        )
    }
}

#[derive(Debug, Error)]
pub enum CommitLogError {
    #[error("generic storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("generic api error: {0}")]
    Api(#[from] ApiError),
    #[error("connection error: {0}")]
    Connection(#[from] xmtp_db::ConnectionError),
    #[error("prost decode error: {0}")]
    Prost(#[from] prost::DecodeError),
    #[error("keystore error: {0}")]
    KeystoreError(#[from] xmtp_db::sql_key_store::SqlKeyStoreError),
    #[error("group error: {0}")]
    GroupError(#[from] GroupError),
    #[error("crypto error: {0}")]
    CryptoError(#[from] openmls_traits::types::CryptoError),
    #[error("try from slice error: {0}")]
    TryFromSliceError(#[from] std::array::TryFromSliceError),
}

impl NeedsDbReconnect for CommitLogError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Api(_api_error) => false,
            Self::Connection(_connection_error) => true, // TODO(cam): verify this is correct
            Self::Prost(_prost_error) => false,
            Self::KeystoreError(_keystore_error) => false, // TODO(rich): What does this method do?
            Self::GroupError(_group_error) => false,       // TODO(rich): What does this method do?
            Self::CryptoError(_crypto_error) => false,
            Self::TryFromSliceError(_try_from_slice_error) => false,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<Context> Worker for CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::CommitLog
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        self.run().await.map_err(|e| Box::new(e) as Box<_>)
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        Self: Sized,
        C: XmtpSharedContext + Send + Sync + 'static,
    {
        Factory { context }
    }
}

pub struct CommitLogWorker<Context> {
    context: Context,
}

impl<Context> CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ConversationCursorInfo {
    pub conversation_id: Vec<u8>,
    pub num_entries_published: usize,
    pub last_entry_published_sequence_id: i64,
    pub last_entry_published_rowid: i64,
}

#[derive(Debug, PartialEq, Clone)]
pub struct SaveRemoteCommitLogResult {
    pub conversation_id: Vec<u8>,
    pub num_entries_saved: usize,
}

// Test related types
#[cfg(test)]
pub enum CommitLogTestFunction {
    PublishCommitLogsToRemote,
    SaveRemoteCommitLog,
    CheckForkedState,
    All,
}

#[cfg(test)]
pub struct TestResult {
    pub save_remote_commit_log_results: Option<HashMap<Vec<u8>, usize>>,
    pub publish_commit_log_results: Option<Vec<ConversationCursorInfo>>,
    pub is_forked: Option<HashMap<Vec<u8>, Option<bool>>>,
}

// CommitLogWorker implementation
impl<Context> CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(&mut self) -> Result<(), CommitLogError> {
        let mut intervals = xmtp_common::time::interval_stream(INTERVAL_DURATION);
        while (intervals.next().await).is_some() {
            self.save_remote_commit_log().await?;
            self.update_forked_state().await?;
            self.publish_commit_logs_to_remote().await?;
        }
        Ok(())
    }

    async fn publish_commit_logs_to_remote(
        &mut self,
    ) -> Result<Vec<ConversationCursorInfo>, CommitLogError> {
        let conn = &self.context.db();
        // Step 1 is to get the list of all group_id for dms and for groups where we are a super admin
        let conversation_ids_for_remote_log_publish =
            conn.get_conversation_ids_for_remote_log_publish()?;

        // Step 2 is to prepare commit log entries for publishing along with the updated cursor for each conversation on publication success
        let (conversation_cursor_info, all_entries) =
            self.prepare_publish_commit_log_info(conn, &conversation_ids_for_remote_log_publish)?;

        // Skip API call if there are no entries to publish
        if all_entries.is_empty() {
            tracing::debug!("No commit log entries to publish");
            return Ok(conversation_cursor_info);
        }

        // Step 3 is to publish commit log entries to the API and update cursors
        let api = self.context.api();
        match api.publish_commit_log(all_entries).await {
            Ok(_) => {
                // Publishing was successful, let's update every group's cursor
                for conversation_cursor_info in &conversation_cursor_info {
                    conn.update_cursor(
                        &conversation_cursor_info.conversation_id,
                        xmtp_db::refresh_state::EntityKind::CommitLogUpload,
                        conversation_cursor_info.last_entry_published_rowid,
                    )?;
                }
            }
            Err(e) => {
                // In this case we do not update the cursor, so next worker iteration will try again
                tracing::error!(
                    "Failed to publish commit log entries to remote commit log, error: {:?}",
                    e
                );
            }
        }
        Ok(conversation_cursor_info)
    }

    // Check each `conversation_id` for new commit log entries. Return a combined list of all entries for batch publishing,
    // along with the new cursor for each conversation on publication success
    fn prepare_publish_commit_log_info(
        &self,
        conn: &impl DbQuery,
        conversation_keys: &[StoredGroupCommitLogPublicKey],
    ) -> Result<(Vec<ConversationCursorInfo>, Vec<PublishCommitLogRequest>), CommitLogError> {
        let mut conversation_cursor_info: Vec<ConversationCursorInfo> = Vec::new();
        let mut all_entries = Vec::new();
        for conversation in conversation_keys {
            // Step 1: Check each conversation cursors to see if we have new commits that have not been published to remote commit log yet
            let local_commit_log_cursor = conn
                .get_local_commit_log_cursor(&conversation.id)
                .ok()
                .flatten()
                .unwrap_or(0);
            let published_commit_log_cursor = conn
                .get_last_cursor_for_id(
                    &conversation.id,
                    xmtp_db::refresh_state::EntityKind::CommitLogUpload,
                )
                .unwrap_or(0);

            if local_commit_log_cursor as i64 <= published_commit_log_cursor {
                // We have no new commits to publish for this conversation
                continue;
            }

            // Step 2: collect all the commit log entries for this conversation
            // Local commit log entries are returned sorted in ascending order of `rowid`
            // All local commit log will have rowid > 0 since sqlite rowid starts at 1 https://www.sqlite.org/autoinc.html
            let (plaintext_commit_log_entries, rowids): (Vec<PlaintextCommitLogEntry>, Vec<i32>) =
                conn.get_local_commit_log_after_cursor(
                    &conversation.id,
                    published_commit_log_cursor,
                    LocalCommitLogOrder::AscendingByRowid,
                )?
                .iter()
                .map(|log| (PlaintextCommitLogEntry::from(log), log.rowid))
                .unzip();

            // Step 3: Compile the conversation cursor info and all the commit log entries for this conversation
            if let Some(max_rowid) = rowids.into_iter().last() {
                let signed_entries =
                    self.sign_group_logs(conversation, &plaintext_commit_log_entries)?;
                all_entries.extend(signed_entries);
                conversation_cursor_info.push(ConversationCursorInfo {
                    conversation_id: conversation.id.clone(),
                    num_entries_published: plaintext_commit_log_entries.len(),
                    last_entry_published_sequence_id: plaintext_commit_log_entries
                        .last()
                        .map(|e| e.commit_sequence_id as i64)
                        .unwrap_or(0),
                    last_entry_published_rowid: max_rowid as i64,
                });
            }
        }
        Ok((conversation_cursor_info, all_entries))
    }

    fn sign_group_logs(
        &self,
        conversation: &StoredGroupCommitLogPublicKey,
        plaintext_commit_log_entries: &[PlaintextCommitLogEntry],
    ) -> Result<Vec<PublishCommitLogRequest>, CommitLogError> {
        let Some(private_key) = get_or_create_signing_key(&self.context, conversation)? else {
            tracing::warn!(
                "No signing key available for group {:?}",
                hex::encode(&conversation.id)
            );
            return Ok(vec![]);
        };

        let provider = self.context.mls_provider();
        let mut signed_entries = Vec::new();
        for entry in plaintext_commit_log_entries {
            let serialized_commit_log_entry = entry.encode_to_vec();
            let signature = provider.crypto().sign(
                SignatureScheme::ED25519,
                &serialized_commit_log_entry,
                private_key.as_slice(),
            )?;
            let public_key = xmtp_cryptography::signature::to_public_key(&private_key)?.to_vec();

            signed_entries.push(PublishCommitLogRequest {
                group_id: conversation.id.clone(),
                serialized_commit_log_entry,
                signature: Some(RecoverableEd25519Signature {
                    bytes: signature,
                    public_key,
                }),
            });
        }
        Ok(signed_entries)
    }

    // Returns a map of conversation_id to the number of entries saved
    async fn save_remote_commit_log(&mut self) -> Result<HashMap<Vec<u8>, usize>, CommitLogError> {
        let conn = &self.context.db();
        // This should be all groups we are in, and all dms are in except sync groups
        let conversation_id_to_public_key: HashMap<Vec<u8>, Option<Vec<u8>>> = conn
            .get_conversation_ids_for_remote_log_download()?
            .into_iter()
            .map(|c| (c.id, c.commit_log_public_key))
            .collect();

        // Step 1 is to collect a list of remote log cursors for all conversations and convert them into query log requests
        let remote_log_cursors = conn.get_remote_log_cursors(
            conversation_id_to_public_key
                .keys()
                .collect::<Vec<_>>()
                .as_slice(),
        )?;
        // For now we will rely on next iteration of the worker to download the next batch of commit log entries
        // if there is more than MAX_PAGE_SIZE entries to download per group
        let query_log_requests: Vec<QueryCommitLogRequest> = remote_log_cursors
            .iter()
            .map(|(conversation_id, cursor)| QueryCommitLogRequest {
                group_id: conversation_id.clone(),
                paging_info: Some(PagingInfo {
                    direction: SortDirection::Ascending as i32,
                    id_cursor: *cursor as u64,
                    limit: remote_commit_log::MAX_PAGE_SIZE,
                }),
            })
            .collect();

        // Skip API call if there are no requests to make
        if query_log_requests.is_empty() {
            tracing::info!("No commit log requests to query");
            return Ok(HashMap::new());
        }

        // Step 2 execute the api call to query remote commit log entries
        let api = self.context.api();
        let query_commit_log_responses = api.query_commit_log(query_log_requests).await?;

        // Step 3 save the remote commit log entries to the local saved remote commit log
        let mut save_remote_commit_log_results = HashMap::new();
        for response in query_commit_log_responses {
            let group_id = response.group_id.clone();
            let mut consensus_public_key: Option<Vec<u8>> = conversation_id_to_public_key
                .get(&group_id)
                .and_then(Option::clone);
            if consensus_public_key.is_none() {
                consensus_public_key =
                    derive_consensus_public_key(&self.context, &response).await?;
            }
            let num_entries = self.save_remote_commit_log_entries_and_update_cursors(
                conn,
                response,
                consensus_public_key,
            )?;
            save_remote_commit_log_results.insert(group_id, num_entries);
        }

        Ok(save_remote_commit_log_results)
    }

    fn save_remote_commit_log_entries_and_update_cursors(
        &self,
        conn: &impl DbQuery,
        commit_log_response: QueryCommitLogResponse,
        consensus_public_key: Option<Vec<u8>>,
    ) -> Result<usize, CommitLogError> {
        let group_id = commit_log_response.group_id;
        let mut num_entries_saved = 0;
        // From the stored remote commit log, fetch the following info:
        // 1. The latest applied epoch authenticator
        // 2. The latest applied epoch number
        // 3. The latest stored sequence id
        if let Some(consensus_public_key) = consensus_public_key {
            let mut latest_saved_remote_log = conn.get_latest_remote_log_for_group(&group_id)?;
            for entry in &commit_log_response.commit_log_entries {
                let commit_log_entry: &CommitLogEntry = entry;
                let log_entry = match PlaintextCommitLogEntry::decode(
                    commit_log_entry.serialized_commit_log_entry.as_slice(),
                ) {
                    Ok(entry) => entry,
                    Err(error) => {
                        tracing::warn!(
                            ?group_id,
                            ?error,
                            "failed to decode commit-log entry, skipping"
                        );
                        continue;
                    }
                };
                if self.should_skip_remote_commit_log_entry(
                    &group_id,
                    latest_saved_remote_log.clone(),
                    commit_log_entry,
                    &log_entry,
                    &consensus_public_key,
                ) {
                    continue;
                }

                num_entries_saved += 1;
                NewRemoteCommitLog {
                    log_sequence_id: commit_log_entry.sequence_id as i64,
                    group_id: log_entry.group_id.clone(),
                    commit_sequence_id: log_entry.commit_sequence_id as i64,
                    commit_result: CommitResult::from(
                        ProtoCommitResult::try_from(log_entry.commit_result)
                            .unwrap_or(ProtoCommitResult::Unspecified),
                    ),
                    applied_epoch_number: log_entry.applied_epoch_number as i64,
                    applied_epoch_authenticator: log_entry.applied_epoch_authenticator.clone(),
                }
                .store(conn)?;

                latest_saved_remote_log = Some(RemoteCommitLog {
                    rowid: 0,
                    log_sequence_id: commit_log_entry.sequence_id as i64,
                    group_id: log_entry.group_id,
                    commit_sequence_id: log_entry.commit_sequence_id as i64,
                    commit_result: CommitResult::from(
                        ProtoCommitResult::try_from(log_entry.commit_result)
                            .unwrap_or(ProtoCommitResult::Unspecified),
                    ),
                    applied_epoch_number: log_entry.applied_epoch_number as i64,
                    applied_epoch_authenticator: log_entry.applied_epoch_authenticator,
                });
            }
        }
        if let Some(last_entry) = commit_log_response.commit_log_entries.last() {
            conn.update_cursor(
                &group_id,
                xmtp_db::refresh_state::EntityKind::CommitLogDownload,
                last_entry.sequence_id as i64,
            )?;
        }

        Ok(num_entries_saved)
    }

    // Should skip if:
    // 1. The entry signature is invalid - TODO(cam)
    // 2. The group_id of the entry does not match the requested group_id.
    // 3. The commit_sequence_id of the entry is <= 0.
    // 4. The commit_sequence_id of the entry is not greater than the most recently stored entry, if one exists.
    // 5. The last_epoch_authenticator does not match the epoch_authenticator of the most recently stored entry with a CommitResult of COMMIT_RESULT_APPLIED, if one exists.
    // 6. The entry has a CommitResult of COMMIT_RESULT_APPLIED, but the epoch number is not exactly 1 greater than the most recently stored entry with a result of COMMIT_RESULT_APPLIED, if one exists.
    // 7. The entry CommitResult is not COMMIT_RESULT_APPLIED, and the epoch authenticator or epoch number does not match the most recently applied values
    fn should_skip_remote_commit_log_entry(
        &self,
        group_id: &[u8],
        latest_saved_remote_log: Option<RemoteCommitLog>,
        serialized_entry: &CommitLogEntry,
        entry: &PlaintextCommitLogEntry,
        consensus_public_key: &[u8],
    ) -> bool {
        // These checks apply even if there is no latest saved remote log
        if entry.group_id != group_id || entry.commit_sequence_id == 0 {
            return true;
        }
        let provider = self.context.mls_provider();
        if provider
            .crypto()
            .verify_commit_log_signature(serialized_entry, consensus_public_key)
            .is_err()
        {
            tracing::warn!(
                "Invalid signature for commit log entry {} on group {}, skipping",
                serialized_entry.sequence_id,
                hex::encode(&entry.group_id),
            );
            return true;
        }

        let Some(latest_saved_remote_log) = latest_saved_remote_log else {
            return false;
        };

        let is_applied = entry.commit_result == ProtoCommitResult::Applied as i32;

        entry.commit_sequence_id <= latest_saved_remote_log.commit_sequence_id as u64
            || (is_applied
                && !latest_saved_remote_log
                    .applied_epoch_authenticator
                    .is_empty()
                && entry.last_epoch_authenticator
                    != latest_saved_remote_log.applied_epoch_authenticator)
            || (is_applied
                && entry.applied_epoch_number as i64
                    != latest_saved_remote_log.applied_epoch_number + 1)
            || (!is_applied
                && (entry.applied_epoch_authenticator
                    != latest_saved_remote_log.applied_epoch_authenticator
                    || entry.applied_epoch_number as i64
                        != latest_saved_remote_log.applied_epoch_number))
    }

    // Updates fork status for conversations in the database
    pub async fn update_forked_state(&mut self) -> Result<(), CommitLogError> {
        let conn = &self.context.db();
        let conversation_ids_for_forked_state_check = conn.get_conversation_ids_for_fork_check()?;

        for conversation_id in conversation_ids_for_forked_state_check {
            let is_forked = self.check_conversation_fork_state(conn, &conversation_id)?;
            // Persist the fork status to the database
            conn.set_group_commit_log_forked_status(&conversation_id, is_forked)?;
        }

        Ok(())
    }

    // Test helper to get fork status for a specific group
    #[cfg(test)]
    pub fn get_group_fork_status(&self, group_id: &[u8]) -> Result<Option<bool>, CommitLogError> {
        let conn = &self.context.db();
        Ok(conn.get_group_commit_log_forked_status(group_id)?)
    }

    // Test helper to get fork status for all groups that would be checked (for backward compatibility with tests)
    #[cfg(test)]
    pub fn get_all_fork_statuses(&self) -> Result<HashMap<Vec<u8>, Option<bool>>, CommitLogError> {
        use xmtp_db::group::GroupQueryArgs;
        let conn = &self.context.db();
        // Get all groups (not just those with commit log keys)
        let all_groups = conn.find_groups(GroupQueryArgs::default())?;

        let mut results = HashMap::new();
        for group in all_groups {
            let fork_status = conn.get_group_commit_log_forked_status(&group.id)?;
            results.insert(group.id, fork_status);
        }

        Ok(results)
    }

    fn check_conversation_fork_state(
        &self,
        conn: &impl DbQuery,
        conversation_id: &[u8],
    ) -> Result<Option<bool>, CommitLogError> {
        // Get cursors for this conversation
        let fork_check_local_cursor = conn.get_last_cursor_for_id(
            conversation_id,
            xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
        )?;
        let fork_check_remote_cursor = conn.get_last_cursor_for_id(
            conversation_id,
            xmtp_db::refresh_state::EntityKind::CommitLogForkCheckRemote,
        )?;

        // Get local and remote commit logs
        let local_logs = conn.get_local_commit_log_after_cursor(
            conversation_id,
            fork_check_local_cursor,
            LocalCommitLogOrder::DescendingByRowid,
        )?;
        let remote_logs =
            conn.get_remote_commit_log_after_cursor(conversation_id, fork_check_remote_cursor)?;

        let mut is_remote_log_up_to_date = true;
        // Check each local log against remote logs for matching commit_sequence_id
        for local_log in &local_logs {
            let Some(matching_remote_log) =
                self.find_matching_remote_log(&remote_logs, local_log.commit_sequence_id)
            else {
                is_remote_log_up_to_date = false;
                continue;
            };
            // Found a matching commit_sequence_id - check if forked
            let is_mismatched = local_log.applied_epoch_authenticator
                != matching_remote_log.applied_epoch_authenticator;

            if is_mismatched {
                tracing::warn!(
                    "Detected forked state for conversation_id: {:?}\n\
                            Local log: {:?}\n\
                            Remote log: {:?}",
                    conversation_id,
                    local_log,
                    matching_remote_log
                );
            }

            // Update cursors regardless of fork status (we found a match)
            conn.update_cursor(
                conversation_id,
                xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
                local_log.rowid as i64,
            )?;
            conn.update_cursor(
                conversation_id,
                xmtp_db::refresh_state::EntityKind::CommitLogForkCheckRemote,
                matching_remote_log.rowid as i64,
            )?;

            if is_mismatched {
                return Ok(Some(true));
            } else if is_remote_log_up_to_date {
                return Ok(Some(false));
            } else {
                // If we haven't verified the latest commit local commit logs, we
                // don't know if we are forked or not
                return Ok(None);
            }
        }

        Ok(None)
    }

    fn find_matching_remote_log<'a>(
        &self,
        remote_logs: &'a [xmtp_db::remote_commit_log::RemoteCommitLog],
        commit_sequence_id: i64,
    ) -> Option<&'a xmtp_db::remote_commit_log::RemoteCommitLog> {
        remote_logs
            .iter()
            .find(|remote_log| remote_log.commit_sequence_id == commit_sequence_id)
    }

    /// Test-only version that runs without infinite loop
    #[cfg(test)]
    pub async fn run_test(
        &mut self,
        commit_log_test_function: CommitLogTestFunction,
        iterations: Option<usize>,
    ) -> Result<Vec<TestResult>, CommitLogError> {
        let mut test_results = Vec::new();
        match iterations {
            Some(n) => {
                // Run exactly n times
                for _ in 0..n {
                    let test_result = self.test_helper(&commit_log_test_function).await?;
                    test_results.push(test_result);
                }
            }
            None => {
                let test_result = self.test_helper(&commit_log_test_function).await?;
                test_results.push(test_result);
            }
        }
        Ok(test_results)
    }

    #[cfg(test)]
    async fn test_helper(
        &mut self,
        commit_log_test_function: &CommitLogTestFunction,
    ) -> Result<TestResult, CommitLogError> {
        let mut test_result = TestResult {
            save_remote_commit_log_results: None,
            publish_commit_log_results: None,
            is_forked: None,
        };
        match commit_log_test_function {
            CommitLogTestFunction::PublishCommitLogsToRemote => {
                let publish_commit_log_results = self.publish_commit_logs_to_remote().await?;
                test_result.publish_commit_log_results = Some(publish_commit_log_results);
            }
            CommitLogTestFunction::SaveRemoteCommitLog => {
                let save_remote_commit_log_results = self.save_remote_commit_log().await?;
                test_result.save_remote_commit_log_results = Some(save_remote_commit_log_results);
            }
            CommitLogTestFunction::CheckForkedState => {
                self.update_forked_state().await?;
                let is_forked = self.get_all_fork_statuses()?;
                test_result.is_forked = Some(is_forked);
            }
            CommitLogTestFunction::All => {
                let publish_commit_log_results = self.publish_commit_logs_to_remote().await?;
                test_result.publish_commit_log_results = Some(publish_commit_log_results);
                let save_remote_commit_log_results = self.save_remote_commit_log().await?;
                test_result.save_remote_commit_log_results = Some(save_remote_commit_log_results);
                self.update_forked_state().await?;
                let is_forked = self.get_all_fork_statuses()?;
                test_result.is_forked = Some(is_forked);
            }
        }
        Ok(test_result)
    }

    #[cfg(test)]
    pub(crate) fn should_skip_remote_commit_log_entry_test(
        &self,
        group_id: &[u8],
        latest_saved_remote_log: Option<RemoteCommitLog>,
        serialized_entry: &xmtp_proto::xmtp::mls::message_contents::CommitLogEntry,
        entry: &PlaintextCommitLogEntry,
        consensus_public_key: &[u8],
    ) -> bool {
        self.should_skip_remote_commit_log_entry(
            group_id,
            latest_saved_remote_log,
            serialized_entry,
            entry,
            consensus_public_key,
        )
    }
}
