//! Leave requests, delete messages, and pending-remove bookkeeping.

use super::*;

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    pub(super) fn process_own_leave_request_message(
        &self,
        mls_group: &OpenMlsGroup,
        storage: &impl XmtpMlsStorageProvider,
        message_id: &[u8],
    ) {
        if let Ok(Some(message)) = storage.db().get_group_message(message_id)
            && message.content_type == ContentType::LeaveRequest
        {
            match self.process_leave_request_message(mls_group, storage, &message, None) {
                Ok(()) => {
                    debug!("Successfully processed leave request message");
                }
                Err(e) => {
                    debug!("Failed to process leave request message: {}", e);
                }
            }
        }
    }

    pub(super) fn process_own_delete_message(
        &self,
        storage: &impl XmtpMlsStorageProvider,
        message_id: &[u8],
    ) {
        let db = storage.db();

        let Ok(Some(message)) = db.get_group_message(message_id) else {
            return;
        };

        if message.content_type != ContentType::DeleteMessage {
            return;
        }

        let Ok(Some(deletion)) = db.get_message_deletion(message_id) else {
            tracing::warn!(
                message_id = hex::encode(message_id),
                "Deletion record not found for own delete message"
            );
            return;
        };

        let Ok(Some(original_msg)) = db.get_group_message(&deletion.deleted_message_id) else {
            tracing::debug!(
                deleted_message_id = hex::encode(&deletion.deleted_message_id),
                "Original message not found for deletion event (may be out-of-order)"
            );
            return;
        };

        let _ = self
            .context
            .local_events()
            .send(crate::subscriptions::LocalEvents::MsgsDeleted(vec![
                original_msg,
            ]));
    }

    pub(super) fn process_leave_request_message(
        &self,
        mls_group: &OpenMlsGroup,
        storage: &impl XmtpMlsStorageProvider,
        message: &StoredGroupMessage,
        deferred_events: Option<&mut DeferredEvents>,
    ) -> Result<(), GroupMessageProcessingError> {
        let current_inbox_id = self.context.inbox_id().to_string();

        // Process leave-request messages - only if the actor is the current user
        // changes if they were made by the same inbox-id
        if message.sender_inbox_id == current_inbox_id {
            storage
                .db()
                .update_group_membership(self.group_id, GroupMembershipState::PendingRemove)?;
        }

        // put the user in the pending-remove list
        PendingRemove {
            group_id: message.group_id,
            inbox_id: message.sender_inbox_id.clone(),
            message_id: message.id.clone(),
        }
        .store_or_ignore(&storage.db())?;

        // Durable backstop: enqueue a self-remove task in THIS txn (atomic with the
        // PendingRemove insert, one per group, survives restart). The inline call
        // below is the fast path; the task is the retry/backstop.
        let now = xmtp_common::time::now_ns();
        let proto = TaskProto {
            task: Some(TaskKind::ProcessPendingSelfRemove(
                ProcessPendingSelfRemove {
                    group_id: self.group_id.to_vec(),
                },
            )),
        };
        let task = xmtp_db::tasks::NewTask::builder()
            .originating_message_sequence_id(message.sequence_id)
            .created_at_ns(now)
            .next_attempt_at_ns(now)
            .build(proto)?;
        storage
            .db()
            .upsert_pending_self_remove_task(&self.group_id, task)?;
        // Wake post-commit. The own-leave path has no DeferredEvents (its task is a
        // no-op) — fine, it's picked up on the worker's next turn.
        if let Some(deferred_events) = deferred_events {
            deferred_events.wake_worker(WorkerKind::TaskRunner);
        }

        // If we reach here, the action was by another user or no validated commit
        // Only process admin actions if we're admin/super-admin
        self.process_admin_pending_remove_actions(mls_group, storage)?;

        Ok(())
    }

    /// Process an incoming DeleteMessage from the network.
    ///
    /// Returns `Ok(())` for invalid deletions to avoid disrupting sync.
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn process_delete_message(
        &self,
        mls_group: &OpenMlsGroup,
        storage: &impl XmtpMlsStorageProvider,
        message: &StoredGroupMessage,
    ) -> Result<(), GroupMessageProcessingError> {
        let encoded_content =
            match EncodedContent::decode(message.decrypted_message_bytes.as_slice()) {
                Ok(content) => content,
                Err(err) => {
                    tracing::warn!(
                        error = ?err,
                        "Failed to decode EncodedContent for delete message, skipping"
                    );
                    return Ok(());
                }
            };

        let delete_msg = match DeleteMessage::decode(encoded_content.content.as_slice()) {
            Ok(msg) => msg,
            Err(err) => {
                tracing::warn!(error = ?err, "Failed to decode DeleteMessage, skipping");
                return Ok(());
            }
        };

        let target_message_id = match hex::decode(&delete_msg.message_id) {
            Ok(id) => id,
            Err(_) => {
                tracing::warn!("Invalid delete message_id: {}", delete_msg.message_id);
                return Ok(());
            }
        };

        let original_msg_opt = storage.db().get_group_message(&target_message_id)?;

        let is_super_admin_deletion = if let Some(ref original_msg) = original_msg_opt {
            if original_msg.group_id.as_slice() != self.group_id.as_slice() {
                tracing::warn!(
                    "Cross-group deletion attempt: message {} from group {}",
                    delete_msg.message_id,
                    hex::encode(original_msg.group_id)
                );
                return Ok(());
            }

            if !original_msg.kind.is_deletable() || !original_msg.content_type.is_deletable() {
                tracing::warn!(
                    "Non-deletable message {} (kind: {:?}, content_type: {:?})",
                    delete_msg.message_id,
                    original_msg.kind,
                    original_msg.content_type
                );
                return Ok(());
            }

            let is_sender = original_msg.sender_inbox_id == message.sender_inbox_id;
            let is_super_admin_deletion = if is_sender {
                false
            } else {
                self.is_super_admin_without_lock(mls_group, message.sender_inbox_id.clone())
                    .unwrap_or(false)
            };

            let is_authorized = is_sender || is_super_admin_deletion;
            if !is_authorized {
                tracing::warn!(
                    "Unauthorized deletion by {} for message {}",
                    message.sender_inbox_id,
                    delete_msg.message_id
                );
                return Ok(());
            }

            is_super_admin_deletion
        } else {
            // Out-of-order: deletion arrived before the message.
            // Authorization is validated at enrichment time via is_deletion_valid().
            self.is_super_admin_without_lock(mls_group, message.sender_inbox_id.clone())
                .unwrap_or(false)
        };

        let deletion = StoredMessageDeletion {
            id: message.id.clone(),
            group_id: self.group_id,
            deleted_message_id: target_message_id.clone(),
            deleted_by_inbox_id: message.sender_inbox_id.clone(),
            is_super_admin_deletion,
            deleted_at_ns: message.sent_at_ns,
        };

        deletion.store_or_ignore(&storage.db())?;

        let out_of_order = original_msg_opt.is_none();
        if let Some(original_msg) = original_msg_opt {
            let _ =
                self.context
                    .local_events()
                    .send(crate::subscriptions::LocalEvents::MsgsDeleted(vec![
                        original_msg,
                    ]));
        }

        tracing::info!(
            "Message {} deleted by {} (super_admin: {}, out_of_order: {})",
            delete_msg.message_id,
            message.sender_inbox_id,
            is_super_admin_deletion,
            out_of_order
        );

        Ok(())
    }

    fn process_admin_pending_remove_actions(
        &self,
        mls_group: &OpenMlsGroup,
        storage: &impl XmtpMlsStorageProvider,
    ) -> Result<(), GroupMessageProcessingError> {
        let current_inbox_id = self.context.inbox_id().to_string();

        // Process admin actions based on current group state
        // If the current user is super-admin and there are pending remove requests, mark the group accordingly
        let is_super_admin = match self
            .is_super_admin_without_lock(mls_group, self.context.inbox_id().to_string())
        {
            Ok(is_admin) => is_admin,
            Err(e) => {
                debug!(
                    "Failed to check super admin status while processing LeaveRequestMessage: {}. Skipping admin pending remove actions.",
                    e
                );
                return Ok(());
            }
        };
        // Only process if we're an admin/super-admin
        if !is_super_admin {
            return Ok(());
        }
        let pending_remove_users = storage
            .db()
            .get_pending_remove_users(&GroupId::try_from(mls_group.group_id())?)?;
        if pending_remove_users.is_empty() {
            return Ok(());
        }

        // if the current user is in pending remove-users, then we should not mark it for the worker
        if !pending_remove_users.contains(&current_inbox_id) {
            self.update_group_pending_status(storage, true)
        }

        Ok(())
    }

    pub(super) fn clean_pending_remove_list(
        &self,
        storage: &impl XmtpMlsStorageProvider,
        removed_inboxes: &[Inbox],
    ) {
        if removed_inboxes.is_empty() {
            return;
        }

        let removed_inbox_ids: Vec<String> = removed_inboxes
            .iter()
            .map(|inbox| inbox.inbox_id.clone())
            .collect();

        match storage
            .db()
            .delete_pending_remove_users(&self.group_id, removed_inbox_ids.clone())
        {
            Ok(_) => {
                tracing::info!(
                    group_id = %self.group_id,
                    removed_inboxes = ?removed_inbox_ids,
                    "Successfully removed left/removed members from pending_remove list"
                );
            }
            Err(e) => {
                tracing::info!(
                    group_id = %self.group_id,
                    removed_inboxes = ?removed_inbox_ids,
                    error = %e,
                    "Failed to clean pending_remove list for removed members"
                );
            }
        }
    }

    pub(super) fn handle_super_admin_status_change(
        &self,
        storage: &impl XmtpMlsStorageProvider,
        mls_group: &OpenMlsGroup,
        metadata_info: &MetadataChanges,
    ) {
        let current_inbox_id = self.context.inbox_id().to_string();

        // Check if current user was promoted to super_admin
        let was_promoted = metadata_info
            .super_admins_added
            .iter()
            .any(|inbox| inbox.inbox_id == current_inbox_id);

        // Check if current user was demoted from super_admin
        let was_demoted = metadata_info
            .super_admins_removed
            .iter()
            .any(|inbox| inbox.inbox_id == current_inbox_id);

        if !was_promoted && !was_demoted {
            // No change in super_admin status for current user
            return;
        }

        if was_promoted {
            // Promoted to super_admin: check if there are pending remove users
            let Ok(group_id) = GroupId::try_from(mls_group.group_id()) else {
                tracing::warn!("Invalid group_id length while handling super-admin promotion");
                return;
            };
            match storage.db().get_pending_remove_users(&group_id) {
                Ok(pending_remove_users) => {
                    if !pending_remove_users.is_empty()
                        && !pending_remove_users.contains(&current_inbox_id)
                    {
                        self.update_group_pending_status(storage, true);
                    }
                }
                Err(e) => {
                    tracing::info!(
                        group_id = %self.group_id,
                        inbox_id = %current_inbox_id,
                        error = %e,
                        "Failed to get pending remove users after promotion"
                    );
                }
            }
        } else if was_demoted {
            // Demoted from super_admin: clear the pending leave request status
            self.update_group_pending_status(storage, false);
        }
    }

    pub(crate) fn update_group_pending_status(
        &self,
        storage: &impl XmtpMlsStorageProvider,
        has_pending_removes: bool,
    ) {
        // This is where we would mark the group as having/not having pending remove requests
        if has_pending_removes {
            tracing::info!(
                group_id = %self.group_id,
                inbox_id = %self.context.inbox_id(),
                "Group has pending remove requests requiring admin action"
            );

            if let Err(e) = storage
                .db()
                .set_group_has_pending_leave_request_status(&self.group_id, Some(true))
            {
                tracing::error!(
                    error = %e,
                    operation = "set_group_pending_status",
                    group_id = %self.group_id,
                    "Failed to mark group as having pending leave requests"
                );
            }
        } else {
            tracing::debug!(
                group_id = %self.group_id,
                inbox_id = %self.context.inbox_id(),
                "Group has no pending remove requests"
            );

            if let Err(e) = storage
                .db()
                .set_group_has_pending_leave_request_status(&self.group_id, Some(false))
            {
                tracing::error!(
                    operation = "set_group_pending_status",
                    group_id = %self.group_id,
                    "Failed to mark group as not having pending leave requests {}",
                    e,
                );
            }
        }
    }

    pub(crate) fn mark_readd_requests_as_responded(
        storage: &impl XmtpMlsStorageProvider,
        group_id: &GroupId,
        readded_installations: &HashSet<Vec<u8>>,
        cursor: i64,
    ) -> Result<(), StorageError> {
        for installation_id in readded_installations {
            storage.db().update_responded_at_sequence_id(
                group_id,
                installation_id.as_slice(),
                cursor,
            )?;
        }
        Ok(())
    }
}
