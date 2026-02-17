mod utils;
pub use utils::*;

#[xmtp_macro::build_logging_metadata]
pub enum Event {
    // ===================== General Client =====================
    /// Client created.
    #[context(device_sync_enabled, disabled_workers, inbox_id, full_installation_id)]
    ClientCreated,
    /// Client dropped.
    ClientDropped,
    /// Associating name with installation.
    #[context(name)]
    AssociateName,

    // ===================== Group Operations =====================
    /// DM created.
    #[context(group_id, target_inbox)]
    CreatedDM,
    /// Group created.
    #[context(group_id)]
    CreatedGroup,
    /// Added members to group.
    #[context(group_id, members, epoch, icon = "➕")]
    AddedMembers,
    /// Received new group from welcome.
    #[context(group_id, conversation_type, epoch, icon = "🤝")]
    ProcessedWelcome,

    // ===================== MLS Operations =====================
    /// Received staged commit. Merging and clearing any pending commits.
    #[context(group_id, sender_installation_id, msg_epoch, epoch, icon = "❗")]
    MLSReceivedStagedCommit,
    /// Processed staged commit.
    #[context(
        group_id,
        actor_installation_id,
        epoch,
        added_inboxes,
        removed_inboxes,
        left_inboxes,
        metadata_changes,
        icon = "😮‍💨"
    )]
    MLSProcessedStagedCommit,
    /// Received application message.
    #[context(group_id, epoch, msg_epoch, sender_inbox_id)]
    MLSReceivedApplicationMessage,
    /// Group epoch updated.
    #[context(group_id, cursor, originator, epoch, epoch_auth, previous_epoch)]
    MLSGroupEpochUpdated,

    // ===================== Network =====================
    /// Stream started.
    #[context(kind)]
    StreamOpened,
    /// Stream closed.
    #[context(kind)]
    StreamClosed,

    // ===================== Group Syncing =====================
    /// Begin syncing group.
    #[context(group_id, icon = "🔄")]
    GroupSyncStart,
    /// Syncing group.
    #[context(group_id, attempt, backoff, icon = "🔃")]
    GroupSyncAttempt,
    /// Group sync complete.
    #[context(group_id, summary, success, icon = "✅")]
    GroupSyncFinished,
    /// Attempted to sync on an inactive group.
    #[context(group_id, icon = "⏸️")]
    GroupSyncGroupInactive,
    /// Intent failed to sync but did not error. This can happen for a variety of reasons.
    #[context(group_id, intent_id, intent_kind, state, icon = "🔁")]
    GroupSyncIntentRetry,
    /// Intent was found to be in error after attempting to sync.
    #[context(group_id, intent_id, intent_kind, summary, icon = "⚠️")]
    GroupSyncIntentErrored,
    /// Attempt to publish intent failed.
    #[context(group_id, intent_id, intent_kind, err, icon = "❌")]
    GroupSyncPublishFailed,
    /// Application message published successfully.
    #[context(group_id, intent_id, icon = "📤")]
    GroupSyncApplicationMessagePublishSuccess,
    /// Commit published successfully.
    #[context(group_id, intent_id, intent_kind, commit_hash, icon = "✨")]
    GroupSyncCommitPublishSuccess,
    /// Commit sent. Staged commit is present. Stopping further publishes for this round.
    #[context(group_id, icon = "🛑")]
    GroupSyncStagedCommitPresent,
    /// Updating group cursor.
    #[context(group_id, cursor, originator, icon = "📍")]
    GroupCursorUpdate,

    // ===================== Group Membership =====================
    /// Updated group membership.
    #[context(group_id, added_installations, removed_installations, icon = "🫂")]
    UpdatedGroupMembership,

    // ===================== Device Sync =====================
    /// Device Sync worker initializing.
    #[context(server_url)]
    DeviceSyncInitializing,
    /// Device sync initialized.
    DeviceSyncInitializingFinished,
    /// No primary sync group found.
    DeviceSyncNoPrimarySyncGroup,
    /// Created primary sync group.
    #[context(group_id)]
    DeviceSyncCreatedPrimarySyncGroup,
    /// Sent a sync request.
    #[context(group_id)]
    DeviceSyncSentSyncRequest,
    /// Processing new sync message.
    #[context(msg_type, external, msg_id, group_id)]
    DeviceSyncProcessingMessages,
    /// Failed to process device sync message.
    #[context(msg_id, err)]
    DeviceSyncMessageProcessingError,
    /// Processing sync archive.
    #[context(msg_id, group_id)]
    DeviceSyncArchiveProcessingStart,
    /// Received a V1 sync payload. V1 is no longer supported. Ignoring.
    DeviceSyncV1Archive,
    /// Received a sync archive message, but it was not requested by this instalaltion. Skipping.
    DeviceSyncArchiveNotRequested,
    /// Downloading sync archive.
    DeviceSyncArchiveDownloading,
    /// Sync archive download failure.
    #[context(status, err)]
    DeviceSyncPayloadDownloadFailure,
    /// Beginning archive import.
    DeviceSyncArchiveImportStart,
    /// Finished sync archive import.
    DeviceSyncArchiveImportSuccess,
    /// Archive import failed.
    #[context(err)]
    DeviceSyncArchiveImportFailure,
    /// Attempted to acknowledge a sync request, but it was already acknowledged
    /// by another installation.
    #[context(request_id, acknowledged_by)]
    DeviceSyncRequestAlreadyAcknowledged,
    /// Acknowledged sync request.
    #[context(request_id)]
    DeviceSyncRequestAcknowledged,
    /// Scheduled task to respond to sync request.
    #[context(request_id)]
    DeviceSyncResponseTaskScheduled,
    /// Sending sync archive.
    #[context(group_id, server_url)]
    DeviceSyncArchiveUploadStart,
    /// Failed to send sync archive.
    #[context(group_id, request_id, err)]
    DeviceSyncArchiveUploadFailure,
    /// Archive upload complete.
    #[context(group_id)]
    DeviceSyncArchiveUploadComplete,
    /// Cannot send sync archive. No server_url present.
    #[context(request_id)]
    DeviceSyncNoServerUrl,
}
