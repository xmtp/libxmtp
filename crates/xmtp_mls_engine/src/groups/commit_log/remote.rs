//! Saving remote commit logs: opening fetched entries, skipping the ones a client must
//! not apply, and advancing the cursors.

use super::*;

#[derive(Debug, PartialEq, Clone)]
pub struct SaveRemoteCommitLogResult {
    pub conversation_id: Vec<u8>,
    pub num_entries_saved: usize,
}

impl<Context> CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    // Returns a map of conversation_id to the number of entries saved
    // implements: FORK-030
    pub(super) async fn save_remote_commit_log(
        &mut self,
    ) -> Result<HashMap<Vec<u8>, usize>, CommitLogError> {
        let conn = &self.context.db();
        // All allowed DMs and groups other than sync and restored groups.
        // Each names its commit-log id, stored at creation and join. A group
        // without one has no root, and so no log to read.
        let conversations: HashMap<Vec<u8>, GroupId> = conn
            .get_conversation_ids_for_remote_log_download()?
            .into_iter()
            .filter_map(|conversation| Some((conversation.commit_log_public_key?, conversation.id)))
            .collect();

        // The download cursor stays keyed by group id; that link is local only.
        let remote_log_cursors = conn.get_remote_log_cursors(
            conversations
                .values()
                .map(|group_id| group_id.as_slice())
                .collect::<Vec<_>>()
                .as_slice(),
        )?;
        let query_log_requests: TopicCursor = conversations
            .iter()
            .map(|(commit_log_id, group_id)| {
                let cursor = remote_log_cursors
                    .get(group_id.as_slice())
                    .copied()
                    .unwrap_or_default();
                (Topic::new_commit_log(commit_log_id), cursor)
            })
            .collect();

        // Skip API call if there are no requests to make
        if query_log_requests.is_empty() {
            tracing::debug!("No commit log requests to query");
            return Ok(HashMap::new());
        }

        let mut keys_by_group = HashMap::new();
        let mut cursors = query_log_requests;
        for group_id in conversations.values() {
            if let Some(keys) = self.group_keys(group_id)? {
                keys_by_group.insert(*group_id, keys);
            }
        }
        cursors.retain(|topic, _| {
            conversations
                .get(topic.identifier())
                .is_some_and(|group| keys_by_group.contains_key(group))
        });
        let limits = self.context.incoming_runtime().policy().fetched_limits();
        let mut save_remote_commit_log_results = HashMap::new();
        self.context
            .api()
            .fold_ordered_pages(cursors, limits, |batches, cursors| {
                for batch in batches {
                    let Some(group_id) = conversations.get(batch.topic.identifier()) else {
                        continue;
                    };
                    let Some(keys) = keys_by_group.get(group_id) else {
                        continue;
                    };
                    let entries = batch
                        .envelopes
                        .into_iter()
                        .map(|envelope| {
                            xmtp_api_backend::envelope::decode_commit_log_entry(envelope.decode()?)
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(ApiError::from)?;
                    let (num_entries, held) = self
                        .save_remote_commit_log_entries_and_update_cursors(
                            conn, group_id, keys, entries,
                        )?;
                    *save_remote_commit_log_results
                        .entry(group_id.to_vec())
                        .or_insert(0) += num_entries;
                    if held {
                        // The durable cursor still precedes the held entry. Do not
                        // fetch or evaluate any later page from this topic this turn.
                        cursors.remove(&batch.topic);
                    }
                }
                Ok::<_, CommitLogError>(())
            })
            .await?;

        Ok(save_remote_commit_log_results)
    }

    /// Open, check, and store each envelope read from a conversation's
    /// commit-log topic, in order, then move the download cursor past the
    /// last one. A discarded envelope is read past and never evaluated again.
    fn save_remote_commit_log_entries_and_update_cursors(
        &self,
        conn: &impl DbQuery,
        group_id: &GroupId,
        keys: &GroupKeys,
        entries: Vec<DecodedCommitLogEntry>,
    ) -> Result<(usize, bool), CommitLogError> {
        // implements: FORK-087
        // Read the durable processed position before loading the tree. Commits
        // and their positions share a transaction, so this tree cannot precede
        // the position against which the hold is checked.
        let processed = conn
            .latest_cursor_for_id(
                group_id,
                &[xmtp_db::refresh_state::EntityKind::ApplicationMessage],
            )?
            .0;
        let (active, publishers, boundaries, reader_epoch_floor) =
            xmtp_mls_core::state_tx::state_write(self.context.mls_storage(), |tx| {
                tx.with_group(*group_id, |g, storage| {
                    use crate::groups::mls_ext::CommitLogStorer;
                    Ok::<_, GroupError>((
                        g.is_active(),
                        ForkActors::from_group(g)?.publisher_keys(g)?,
                        storage.db().get_commit_log_publisher_boundaries(group_id)?,
                        g.initialize_commit_log_reader_epoch(&storage.db())?,
                    ))
                })
                .map(Continue)
            })?
            .into_continued();
        let mut evaluated_to = None;
        let mut held = false;
        let mut num_entries_saved = 0;
        // From the stored remote commit log, fetch the following info:
        // 1. The latest applied epoch authenticator
        // 2. The latest applied epoch number
        // 3. The latest stored sequence id
        let mut latest_saved_remote_log = conn.get_latest_remote_log_for_group(group_id)?;
        for decoded in &entries {
            let Some(log_entry) = open_remote_entry(keys, group_id, decoded) else {
                evaluated_to = Some(decoded.cursor);
                continue;
            };
            // implements: FORK-087
            if log_entry.applied_epoch_number < reader_epoch_floor {
                evaluated_to = Some(decoded.cursor);
                continue;
            }
            if active && log_entry.commit_sequence_id > processed {
                held = true;
                break;
            }
            evaluated_to = Some(decoded.cursor);
            if !publishers.contains(&log_entry.author_installation_key) {
                continue;
            }
            let chain_holds = latest_saved_remote_log.as_ref().is_some_and(|latest| {
                let boundary = boundaries
                    .get(&latest.author_installation_key)
                    .copied()
                    .unwrap_or(0);
                publishers.contains(&latest.author_installation_key)
                    && !(latest.commit_sequence_id < boundary
                        && log_entry.commit_sequence_id >= boundary as u64)
            });
            if should_skip_remote_commit_log_entry(
                latest_saved_remote_log.as_ref(),
                &log_entry,
                chain_holds,
            ) {
                continue;
            }

            let commit_result = CommitResult::from(
                ProtoCommitResult::try_from(log_entry.commit_result)
                    .unwrap_or(ProtoCommitResult::Unspecified),
            );
            num_entries_saved += 1;
            NewRemoteCommitLog {
                log_sequence_id: decoded.cursor.0 as i64,
                group_id: *group_id,
                commit_sequence_id: log_entry.commit_sequence_id as i64,
                commit_result,
                applied_epoch_number: log_entry.applied_epoch_number as i64,
                applied_epoch_authenticator: log_entry.applied_epoch_authenticator.clone(),
                author_installation_key: log_entry.author_installation_key.clone(),
            }
            .store(conn)?;

            latest_saved_remote_log = Some(RemoteCommitLog {
                rowid: 0,
                log_sequence_id: decoded.cursor.0 as i64,
                group_id: *group_id,
                commit_sequence_id: log_entry.commit_sequence_id as i64,
                commit_result,
                applied_epoch_number: log_entry.applied_epoch_number as i64,
                applied_epoch_authenticator: log_entry.applied_epoch_authenticator,
                author_installation_key: log_entry.author_installation_key,
            });
        }
        if let Some(cursor) = evaluated_to {
            conn.update_cursor(
                group_id,
                xmtp_db::refresh_state::EntityKind::CommitLogDownload,
                cursor,
            )?;
        }

        Ok((num_entries_saved, held))
    }
}

/// Open one envelope read from a conversation's commit-log topic. `None`
/// discards it; the reason is logged without entry bytes. The log names the
/// envelope by its commit-log id and sequence id only, never by group id, so
/// it does not link the conversation to its commit-log topic.
// implements: FORK-077
fn open_remote_entry(
    keys: &GroupKeys,
    group_id: &GroupId,
    decoded: &DecodedCommitLogEntry,
) -> Option<PlaintextCommitLogEntry> {
    let commit_log_id = keys.commit_log_id();
    let Some(sealed) = &decoded.sealed else {
        tracing::warn!(
            commit_log_id = %commit_log_id.as_slice().snippet(),
            sequence_id = decoded.cursor.0,
            "Discarding a commit log envelope that holds no sealed entry"
        );
        return None;
    };
    open_entry(keys, group_id.as_slice(), decoded.cursor.0, sealed)
        .inspect_err(|reason| {
            tracing::warn!(
                commit_log_id = %commit_log_id.as_slice().snippet(),
                sequence_id = decoded.cursor.0,
                %reason,
                "Discarding a commit log entry"
            );
        })
        .ok()
}

// If the last accepted author lost its publisher role or the entry crosses
// that author's publisher boundary, only rule 1 applies.
// Otherwise an opened entry is skipped if:
// 1. The commit_sequence_id of the entry is not greater than the most recently stored entry, if one exists.
// 2. The last_epoch_authenticator does not match the epoch_authenticator of the most recently stored entry with a CommitResult of COMMIT_RESULT_APPLIED, if one exists.
// 3. The entry has a CommitResult of COMMIT_RESULT_APPLIED, but the epoch number is not exactly 1 greater than the most recently stored entry with a result of COMMIT_RESULT_APPLIED, if one exists.
// 4. The entry CommitResult is not COMMIT_RESULT_APPLIED, and the epoch authenticator or epoch number does not match the most recently applied values
// implements: FORK-083
pub(crate) fn should_skip_remote_commit_log_entry(
    latest_saved_remote_log: Option<&RemoteCommitLog>,
    entry: &PlaintextCommitLogEntry,
    chain_holds: bool,
) -> bool {
    let Some(latest_saved_remote_log) = latest_saved_remote_log else {
        return false;
    };

    if !chain_holds {
        return entry.commit_sequence_id <= latest_saved_remote_log.commit_sequence_id as u64;
    }
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
