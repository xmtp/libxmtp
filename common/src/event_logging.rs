/// Metadata about a log event variant, including its doc comment and required context fields.
/// This struct is used by proc macros to access event metadata at compile time.
#[derive(Debug, Clone, Copy)]
pub struct EventMetadata {
    /// The name of the enum variant
    pub name: &'static str,
    /// The doc comment describing the event
    pub doc: &'static str,
    /// The required context fields for this event
    pub context_fields: &'static [&'static str],
}

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
