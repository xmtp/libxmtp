mod utils;
pub use utils::*;

#[xmtp_macro::build_logging_metadata]
pub enum Event {
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

    // ===================== Group Membership =====================
    /// Fetching installation diff
    #[context(group_id, old_membership, new_membership)]
    MembershipInstallationDiff,
}
