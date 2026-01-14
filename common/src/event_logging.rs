mod utils;
pub use utils::*;

#[xmtp_macro::build_logging_metadata]
pub enum Event {
    // ===================== General Client =====================
    /// Client created
    #[context(inbox_id, device_sync_enabled, disabled_workers)]
    ClientCreated,

    // ===================== Group Operations =====================
    /// DM created.
    #[context(group_id, target_inbox_id)]
    CreatedDM,
    /// Group created.
    #[context(group_id)]
    CreatedGroup,
    /// Added members to group.
    #[context(group_id, members)]
    AddedMembers,
    /// Received new group from welcome.
    #[context(group_id, conversation_type)]
    ProcessedWelcome,

    // ===================== MLS Operations =====================
    /// Received staged commit. Merging and clearing any pending commits.
    #[context(group_id, inbox_id, sender_inbox_id, msg_epoch, current_epoch)]
    MLSReceivedStagedCommit,
    /// Processed staged commit.
    #[context(group_id, current_epoch)]
    MLSProcessedStagedCommit,
    /// Received application message.
    #[context(group_id, current_epoch, msg_epoch, sender_inbox_id)]
    MLSReceivedApplicationMessage,
    /// Processed application message.
    #[context(group_id)]
    MLSProcessedApplicationMessage,
    /// Group epoch updated.
    #[context(group_id, cursor, epoch, previous_epoch)]
    MLSGroupEpochUpdated,

    // ===================== Group Syncing =====================
    /// Begin syncing group.
    #[context(group_id)]
    GroupSyncStart,
    /// Attempting to sync group.
    #[context(group_id, attempt, backoff)]
    GroupSyncAttempt,
    /// Group sync complete.
    #[context(group_id, summary, success)]
    GroupSyncFinished,
    /// Attempted to sync on an inactive group.
    #[context(group_id)]
    GroupSyncGroupInactive,
    /// Intent failed to sync but did not error. This can happen for a variety of reasons.
    #[context(group_id, intent_id, intent_kind, state)]
    GroupSyncIntentRetry,
    /// Intent was found to be in error after attempting to sync.
    #[context(group_id, intent_id, intent_kind, summary)]
    GroupSyncIntentErrored,
    /// Attempt to publish intent failed.
    #[context(group_id, intent_id, intent_kind, err)]
    GroupSyncPublishFailed,
    /// Application message published successfully.
    #[context(group_id, intent_id)]
    GroupSyncApplicationMessagePublishSuccess,
    /// Commit published successfully.
    #[context(group_id, intent_id, intent_kind, commit_hash)]
    GroupSyncCommitPublishSuccess,
    /// Commit sent. Staged commit is present. Stopping further publishes for this round.
    #[context(group_id)]
    GroupSyncStagedCommitPresent,
    /// Updating group cursor.
    #[context(group_id, cursor)]
    GroupCursorUpdate,

    // ===================== Group Membership =====================
    /// Updating group membership. Calculating which installations need to be added / removed.
    #[context(group_id, old_membership, new_membership)]
    MembershipInstallationDiff,
    /// Result: The following installations need to be added / removed.
    #[context(group_id, added_installations, removed_installations)]
    MembershipInstallationDiffComputed,

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
}
