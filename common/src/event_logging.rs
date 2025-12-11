mod utils;
pub use utils::*;

#[xmtp_macro::build_logging_metadata]
pub enum Event {
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
