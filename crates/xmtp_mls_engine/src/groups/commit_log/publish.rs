//! Publishing local commit logs: preparing each conversation, sealing its entries
//! with the group keys, and publishing them to the remote log.

use super::*;

/// One conversation's sealed entries and the cursor that publishing them
/// reaches.
pub(super) struct PendingPublish {
    pub(super) cursor_info: ConversationCursorInfo,
    pub(super) entries: Vec<SealedCommitLogEntry>,
}

impl<Context> CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    /// Publish every eligible conversation's new entries, each conversation
    /// in its own publish call, so a refused or unsendable entry holds back
    /// only its own conversation. A failed publish is logged, recorded in
    /// [`Self::failed_publishes`], and moves the conversation's cursor past
    /// the stored prefix only; the conversation is tried again at the next
    /// tick. A refusal, a rate limit, or a local parse failure holds back
    /// only its own conversation. A transport failure ends the publishing
    /// part of the turn, since every later conversation would wait through
    /// the same retries. No publish failure stops the worker.
    // implements: FORK-021
    pub(super) async fn publish_commit_logs_to_remote(
        &mut self,
    ) -> Result<Vec<ConversationCursorInfo>, CommitLogError> {
        self.failed_publishes.clear();
        let now = xmtp_common::time::Instant::now();
        self.holds.lock().publish.retain(|_, until| now < *until);
        let conn = &self.context.db();
        // DMs, and groups where we are a super admin
        let conversations = conn.get_conversation_ids_for_remote_log_publish()?;
        let api = self.context.api();
        let mut published = Vec::new();
        for conversation in &conversations {
            // A rate-limited publish is not retried at a later tick inside its
            // delay; other conversations still publish.
            // implements: RATE-041
            if self.holds.lock().publish.contains_key(&conversation.id) {
                continue;
            }
            let Some(PendingPublish {
                cursor_info,
                entries,
            }) = self.prepare_conversation_publish(conn, &conversation.id)?
            else {
                continue;
            };
            let mut stored = Vec::new();
            let result = if entries.is_empty() {
                Ok(())
            } else {
                tracing::info!(
                    group_id = %conversation.id.as_slice().snippet(),
                    entries = entries.len(),
                    "Publishing commit log entries to remote commit log"
                );
                // Entries go out in order, one request at a time, and
                // publishing stops at the first failed request.
                api.publish_commit_log(entries, &mut stored).await
            };
            // Move the cursor past the stored entries. After a failure the
            // next turn starts at the first entry not stored.
            if let Some(rowid) = cursor_info.stored_cursor(stored.len()) {
                conn.update_cursor(
                    conversation.id,
                    xmtp_db::refresh_state::EntityKind::CommitLogUpload,
                    Cursor(rowid as u64),
                )?;
            }
            match result {
                Ok(()) => published.push(cursor_info),
                Err(error) => {
                    tracing::warn!(
                        group_id = %conversation.id.as_slice().snippet(),
                        stored = stored.len(),
                        %error,
                        "Commit log publish failed; the conversation retries at the next turn"
                    );
                    self.failed_publishes.push(conversation.id);
                    hold(
                        &mut self.holds.lock().publish,
                        conversation.id,
                        error.rate_limit_delay(),
                    );
                    // A rate limit is retryable but holds only its own
                    // conversation (RATE-041); the others still publish.
                    if error.is_retryable() && error.rate_limit_delay().is_none() {
                        break;
                    }
                }
            }
        }
        Ok(published)
    }

    /// Seal a conversation's entries recorded after its upload cursor, in
    /// ascending row order, with the cursor that publishing them reaches.
    /// `None` when it has no new entries, or when they cannot be sealed this
    /// turn.
    // implements: FORK-021, FORK-085
    pub(super) fn prepare_conversation_publish(
        &self,
        conn: &impl DbQuery,
        conversation_id: &GroupId,
    ) -> Result<Option<PendingPublish>, CommitLogError> {
        // Recovery and its durable cutoff share the MLS state transaction.
        // Enumeration includes unrecovered groups even when their old flag is false.
        let recovery = xmtp_mls_core::state_tx::state_write(self.context.mls_storage(), |tx| {
            tx.with_group(*conversation_id, |group, storage| {
                use crate::groups::mls_ext::CommitLogStorer;
                let db = storage.db();
                let permits = group
                    .recover_commit_log_publisher(&db, self.context.inbox_id().as_ref())
                    .map_err(GroupError::from)?;
                // Bound this batch to the same state that authorized publication.
                // Keep cursor read errors outside the per-conversation recovery fallback.
                Ok::<_, GroupError>((permits, db.get_local_commit_log_cursor(conversation_id)))
            })
            .map(Continue)
        });
        let (permits, local_commit_log_cursor) = match recovery {
            Ok(outcome) => outcome.into_continued(),
            Err(error) if error.needs_db_reconnect() => return Err(error.into()),
            Err(error) => {
                tracing::warn!(
                    group_id = %conversation_id.as_slice().snippet(),
                    %error,
                    "A conversation's publisher state did not load; it skips publication this turn"
                );
                return Ok(None);
            }
        };

        if !permits {
            return Ok(None);
        }
        // Propagate read errors (incl. a dropped pool) to the supervisor; a
        // missing cursor legitimately defaults to 0.
        let local_commit_log_cursor = local_commit_log_cursor?.unwrap_or(0);
        let membership_floor = conn
            .get_latest_chain_start_rowid(conversation_id)?
            .unwrap_or(0) as u64;
        let published_commit_log_cursor = conn
            .get_last_cursor(
                conversation_id,
                xmtp_db::refresh_state::EntityKind::CommitLogUpload,
            )?
            .0
            .max(membership_floor);
        if local_commit_log_cursor <= published_commit_log_cursor as i32 {
            // We have no new commits to publish for this conversation
            return Ok(None);
        }

        // Local commit log entries are returned sorted in ascending order of `rowid`
        // All local commit log will have rowid > 0 since sqlite rowid starts at 1 https://www.sqlite.org/autoinc.html
        let mut logs = conn.get_local_commit_log_after_cursor(
            conversation_id,
            published_commit_log_cursor as i64,
            LocalCommitLogOrder::AscendingByRowid,
        )?;
        logs.retain(|log| log.rowid <= local_commit_log_cursor);
        let Some(max_rowid) = logs.last().map(|log| log.rowid) else {
            return Ok(None);
        };
        // A commit that removed us is recorded locally (RemovedFromGroup,
        // with the pre-commit epoch/authenticator) for debuggability, but
        // must never be published: it does not attest the new epoch and
        // could shadow the real consensus entry published by the
        // remaining members. In practice a removed member is not a
        // publisher (only super admins and DM participants publish, super
        // admins cannot be removed, and DMs have no removals) — this
        // filter makes that property structural. The publish cursor still
        // advances past skipped rows.
        let removed_from_group_marker = CommitType::RemovedFromGroup.to_string();
        let (entry_rowids, plaintext_commit_log_entries): (Vec<i64>, Vec<PlaintextCommitLogEntry>) =
            logs.iter()
                .filter(|log| {
                    log.commit_type.as_deref() != Some(removed_from_group_marker.as_str())
                })
                .map(|log| (log.rowid as i64, PlaintextCommitLogEntry::from(log)))
                .unzip();
        let entries = if plaintext_commit_log_entries.is_empty() {
            Vec::new()
        } else {
            let Some(entries) =
                self.seal_group_logs(conversation_id, &plaintext_commit_log_entries)?
            else {
                return Ok(None);
            };
            entries
        };
        Ok(Some(PendingPublish {
            cursor_info: ConversationCursorInfo {
                conversation_id: conversation_id.to_vec(),
                num_entries_published: plaintext_commit_log_entries.len(),
                last_entry_published_sequence_id: plaintext_commit_log_entries
                    .last()
                    .map(|e| e.commit_sequence_id as i64)
                    .unwrap_or(0),
                last_entry_published_rowid: max_rowid as i64,
                entry_rowids,
            },
            entries,
        }))
    }

    /// Seal entries under the keys of the conversation's group root.
    /// `None` when the conversation has no root or its group state does not
    /// load, or when a seal fails its self-check. Nothing of that
    /// conversation is then published this turn, and the failure is logged
    /// without any entry or key bytes.
    fn seal_group_logs(
        &self,
        conversation_id: &GroupId,
        plaintext_commit_log_entries: &[PlaintextCommitLogEntry],
    ) -> Result<Option<Vec<SealedCommitLogEntry>>, CommitLogError> {
        let Some(keys) = self.group_keys(conversation_id)? else {
            return Ok(None);
        };
        let mut sealed = Vec::with_capacity(plaintext_commit_log_entries.len());
        for entry in plaintext_commit_log_entries {
            let mut entry = entry.clone();
            let sealed_entry = sign_entry_author(
                &keys,
                &self.context.identity().installation_keys,
                &mut entry,
            )
            .and_then(|()| seal_entry(&keys, &entry));
            match self.apply_seal_fault(conversation_id, sealed_entry) {
                Ok(entry) => sealed.push(entry),
                Err(error) => {
                    tracing::error!(
                        group_id = %conversation_id.as_slice().snippet(),
                        %error,
                        "Sealing a commit log entry failed; the conversation publishes nothing this turn"
                    );
                    return Ok(None);
                }
            }
        }
        Ok(Some(sealed))
    }

    #[cfg(not(test))]
    fn apply_seal_fault(
        &self,
        _conversation_id: &GroupId,
        sealed: Result<SealedCommitLogEntry, SealError>,
    ) -> Result<SealedCommitLogEntry, SealError> {
        sealed
    }

    #[cfg(test)]
    fn apply_seal_fault(
        &self,
        conversation_id: &GroupId,
        sealed: Result<SealedCommitLogEntry, SealError>,
    ) -> Result<SealedCommitLogEntry, SealError> {
        match &self.seal_fault {
            Some(fault) => fault(conversation_id, sealed),
            None => sealed,
        }
    }

    /// The keys derived from a conversation's group root, read from its
    /// group context each time and never stored. `None` for a group without
    /// a root, such as a restored placeholder, and for a group whose state
    /// does not load. Either holds back only that conversation. A dropped
    /// database pool is not one conversation's failure, so it is returned.
    pub(super) fn group_keys(
        &self,
        conversation_id: &GroupId,
    ) -> Result<Option<GroupKeys>, CommitLogError> {
        let group_context = MlsGroup::new_cached(self.context.clone(), conversation_id)
            .map_err(GroupError::from)
            .and_then(|(group, _)| group.load_group_context());
        let group_context = match group_context {
            Ok(group_context) => group_context,
            Err(error) if error.needs_db_reconnect() => return Err(error.into()),
            Err(error) => {
                tracing::warn!(
                    group_id = %conversation_id.as_slice().snippet(),
                    %error,
                    "A conversation's group state did not load; it skips the commit log this turn"
                );
                return Ok(None);
            }
        };
        match group_keys_from_extensions(group_context.extensions()) {
            Ok(keys) => Ok(Some(keys)),
            Err(error) => {
                tracing::warn!(
                    group_id = %conversation_id.as_slice().snippet(),
                    %error,
                    "A conversation without a group root has no commit log"
                );
                Ok(None)
            }
        }
    }
}
