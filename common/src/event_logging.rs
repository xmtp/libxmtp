mod utils;
pub use utils::*;

#[xmtp_macro::build_logging_metadata]
pub enum Event {
    /// DM created
    #[context(group_id, target_inbox_id)]
    CreatedDM,
    /// Group created
    #[context(group_id)]
    CreatedGroup,
    /// Added members to group
    #[context(group_id, members)]
    AddMembers,
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
