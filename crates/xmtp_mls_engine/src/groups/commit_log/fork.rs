//! Fork detection: comparing local and remote commit logs and recording a
//! conversation's fork state.

use super::*;

impl<Context> CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    // Updates fork status for conversations in the database
    pub async fn update_forked_state(&mut self) -> Result<(), CommitLogError> {
        let conversation_ids_for_forked_state_check =
            self.context.db().get_conversation_ids_for_fork_check()?;

        for conversation_id in conversation_ids_for_forked_state_check {
            self.refresh_fork_state(&GroupId::try_from(conversation_id)?)?;
            tokio::task::yield_now().await;
        }

        Ok(())
    }

    /// Recompute and persist a conversation's fork status. A stored `forked`
    /// is sticky and returned without recomputing.
    // implements: FORK-041, FORK-082
    pub(super) fn refresh_fork_state(
        &self,
        conversation_id: &GroupId,
    ) -> Result<Option<bool>, CommitLogError> {
        xmtp_mls_core::state_tx::state_write_with_events(
            self.context.mls_storage(),
            self.context.events(),
            |tx, events| {
                let storage = tx.storage();
                let db = storage.db();
                if db.get_group_commit_log_forked_status(conversation_id)? == Some(true) {
                    return Ok::<_, CommitLogError>(Continue(Some(true)));
                }
                let is_forked = self.check_conversation_fork_state(&db, conversation_id)?;
                db.set_group_commit_log_forked_status(conversation_id, is_forked)?;
                if is_forked == Some(true) {
                    let dm_id = db
                        .find_group(conversation_id)?
                        .and_then(|group| group.dm_id);
                    events.emit_with_context(
                        Some(ClientEvent::ConversationForkDetected(GroupRef {
                            group_id: conversation_id.to_vec(),
                        })),
                        None,
                        dm_event_context(dm_id.as_deref()),
                    );
                }
                Ok(Continue(is_forked))
            },
        )
        .map(TransactionOutcome::into_continued)
    }

    fn check_conversation_fork_state(
        &self,
        conn: &impl DbQuery,
        conversation_id: &GroupId,
    ) -> Result<Option<bool>, CommitLogError> {
        // Get cursors for this conversation
        let fork_check_local_cursor = conn.get_last_cursor(
            conversation_id,
            xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
        )?;
        let fork_check_remote_cursor = conn.get_last_cursor(
            conversation_id,
            xmtp_db::refresh_state::EntityKind::CommitLogForkCheckRemote,
        )?;

        // Chain-start anchor: rows with `commit_sequence_id == 0` (Welcome /
        // GroupCreation / BackupRestore) mark the beginning of this member's
        // current membership session — e.g. rejoining via a new welcome after
        // having been removed. History before the latest chain start is not
        // attestable by this member and must not be compared against remote
        // consensus. Those rows are filtered out of
        // `get_local_commit_log_after_cursor`, so the anchor is looked up
        // separately and applied as a floor on the local fork-check cursor.
        let mut local_cursor = fork_check_local_cursor.0 as i64;
        let mut crossed_chain_start = false;
        if let Some(anchor_rowid) = conn.get_latest_chain_start_rowid(conversation_id)?
            && anchor_rowid as i64 > local_cursor
        {
            local_cursor = anchor_rowid as i64;
            crossed_chain_start = true;
            conn.update_cursor(
                conversation_id,
                xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
                Cursor(anchor_rowid as u64),
            )?;
        }

        // Get local and remote commit logs
        let local_logs = conn.get_local_commit_log_after_cursor(
            conversation_id,
            local_cursor,
            LocalCommitLogOrder::DescendingByRowid,
        )?;
        let remote_logs = conn.get_remote_commit_log_after_cursor(
            conversation_id,
            fork_check_remote_cursor.0 as i64,
            RemoteCommitLogOrder::DescendingByRowid,
        )?;

        // If there are no new commits to check, preserve the existing fork status
        if local_logs.is_empty() {
            if crossed_chain_start {
                // A new chain start (e.g. a welcome from being re-added)
                // invalidates any previously computed status: nothing after
                // it has been verified against remote consensus yet.
                return Ok(None);
            }
            return Ok(conn.get_group_commit_log_forked_status(conversation_id)?);
        }

        let mut is_remote_log_up_to_date = true;
        // Check each local log against remote logs for matching commit_sequence_id
        for local_log in &local_logs {
            // Terminal removal marker: a commit that removed us merges only
            // the public diff — we cannot derive the new epoch's secrets, so
            // a Success row whose applied authenticator equals its last
            // authenticator means "this commit removed us", not a fork. The
            // remaining members publish the real post-commit authenticator,
            // which can never match ours, so this row must be excluded from
            // comparison. (Covers both new RemovedFromGroup rows and legacy
            // rows that recorded the new epoch with the stale authenticator.)
            if local_log.commit_result == CommitResult::Success
                && local_log.applied_epoch_authenticator == local_log.last_epoch_authenticator
            {
                // Note: `update_cursor` is monotonic (it only ever moves the
                // cursor forward), so the cursor updates below for older
                // matched rows — the loop walks descending rowids — cannot
                // regress the cursor behind this terminal row.
                conn.update_cursor(
                    conversation_id,
                    xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
                    Cursor(local_log.rowid as u64),
                )?;
                continue;
            }

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
                // No backend sequence id: with the group id it would link the
                // conversation to its commit-log topic.
                tracing::warn!(
                    group_id = %conversation_id.as_slice().snippet(),
                    commit_sequence_id = local_log.commit_sequence_id,
                    "Detected forked state: the local and remote epoch authenticators differ"
                );
            }

            // Update cursors regardless of fork status (we found a match)
            conn.update_cursor(
                conversation_id,
                xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
                Cursor(local_log.rowid as u64),
            )?;
            conn.update_cursor(
                conversation_id,
                xmtp_db::refresh_state::EntityKind::CommitLogForkCheckRemote,
                Cursor(matching_remote_log.rowid as u64),
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
}
