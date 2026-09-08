mod utils;
pub use utils::*;

#[xmtp_macro::build_logging_metadata]
pub enum Event {
    // ===================== General Client =====================
    /// Client created.
    #[context(
        device_sync_enabled,
        disabled_workers,
        inbox_id,
        full_installation_id,
        icon = "⬆️"
    )]
    ClientCreated,
    /// Client dropped.
    #[context(icon = "⬇️")]
    ClientDropped,
    /// Client cleanly closed via `Client::close`.
    #[context(icon = "🔒")]
    ClientClosed,
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
    #[context(group_id, conversation_type, epoch, epoch_auth, icon = "🤝")]
    ReceivedWelcome,

    // ===================== MLS Operations =====================
    /// Received staged commit. Merging and clearing any pending commits.
    #[context(
        group_id,
        sender_installation_id,
        message_epoch,
        epoch,
        hash,
        icon = "❗"
    )]
    MLSReceivedStagedCommit,
    /// Processed staged commit.
    #[context(
        group_id,
        actor_installation_id,
        epoch,
        epoch_auth,
        added_inboxes,
        removed_inboxes,
        left_inboxes,
        metadata_changes,
        cursor,
        icon = "😮‍💨"
    )]
    MLSProcessedStagedCommit,
    /// Received application message.
    #[context(group_id, epoch, message_epoch, sender_inbox_id)]
    MLSReceivedApplicationMessage,

    // ===================== Network =====================
    /// Stream started.
    #[context(kind, icon = "🌊")]
    StreamOpened,
    /// Stream closed.
    #[context(kind, icon = "🏜️")]
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
    /// Intent failed to sync and will be retried.
    #[context(group_id, intent_id, intent_kind, state, icon = "🔁")]
    GroupSyncIntentRetry,
    /// Intent was found to be in error after attempting to sync.
    /// The summary is logged once by `GroupSyncFinished`; this only marks
    /// which intent errored.
    #[context(group_id, intent_id, intent_kind, icon = "⚠️")]
    GroupSyncIntentErrored,
    /// Attempt to publish intent failed.
    #[context(group_id, intent_id, intent_kind, error, icon = "❌")]
    GroupSyncPublishFailed,
    /// Application message published successfully.
    #[context(group_id, intent_id, icon = "📤")]
    GroupSyncApplicationMessagePublishSuccess,
    /// Commit published successfully.
    #[context(group_id, intent_id, intent_kind, commit_hash, icon = "✨")]
    GroupSyncCommitPublishSuccess,
    /// Commit sent. Staged commit is present. Stopping further publishes for this round.
    #[context(group_id, hash, icon = "🛑")]
    GroupSyncStagedCommitPresent,
    /// Updating group cursor.
    #[context(group_id, cursor, icon = "📍")]
    GroupCursorUpdate,

    // ===================== Group Membership =====================
    /// Updated group membership.
    #[context(group_id, added_installations, removed_installations, icon = "🫂")]
    UpdatedGroupMembership,

    // ===================== Device Sync =====================
    /// Device Sync worker initializing.
    DeviceSyncInitializing,
    /// Device sync initialized.
    DeviceSyncInitializingFinished,
    /// No primary sync group found.
    DeviceSyncNoPrimarySyncGroup,
    /// Created primary sync group.
    #[context(group_id)]
    DeviceSyncCreatedPrimarySyncGroup,
    /// Processing new sync message.
    #[context(msg_type, external, message_id, group_id)]
    DeviceSyncProcessingMessages,
    /// Failed to process device sync message.
    #[context(message_id, error)]
    DeviceSyncMessageProcessingError,

    // ===================== AppData Migration =====================
    /// `enable_proposals` started — pre-flight passed, about to publish
    /// the legacy-GMM bump (step A) and/or bootstrap commit (step B).
    #[context(group_id, min_version, force, icon = "🌱")]
    EnableProposalsStart,
    /// `enable_proposals` completed. `already_migrated = true` means
    /// the call was a no-op fast-path; `false` means a bootstrap
    /// commit was actually published.
    #[context(group_id, already_migrated, min_version, icon = "🌳")]
    EnableProposalsCompleted,
}
