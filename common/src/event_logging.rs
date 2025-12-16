mod utils;
pub use utils::*;

#[xmtp_macro::build_logging_metadata]
pub enum Event {
    // ===================== Commit Ops =====================
    /// Installation missing from AssociationState while validating commit.
    /// If this is
    #[context(inbox_id)]
    CommitValidationMissingInstallation,

    // ===================== Group Ops =====================
    /// DM created.
    #[context(group_id, target_inbox_id)]
    CreatedDM,
    /// Group created.
    #[context(group_id)]
    CreatedGroup,
    /// Added members to group.
    #[context(group_id, members)]
    AddedMembers,

    // ===================== Group Syncing =====================
    /// Begin syncing group.
    #[context(group_id)]
    GroupSyncStart,
    /// Group sync complete.
    #[context(group_id, summary, success)]
    GroupSyncFinished,
    /// Attempted to sync on an inactive group.
    #[context(group_id)]
    GroupSyncGroupInactive,
    /// Intent failed to sync but did not error. This can happen for a variety of reasons.
    #[context(group_id, intent_id, state)]
    GroupSyncIntentRetry,
    /// Intent was found to be in error after attempting to sync.
    #[context(group_id, intent_id, summary)]
    GroupSyncIntentErrored,

    // ===================== MLS Ops =====================
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
}
