use crate::groups::{mls_sync::GroupMessageProcessingError, validated_commit::ValidatedCommit};
use openmls::group::{MlsGroup, StagedCommit};
use xmtp_db::{
    group_intent::IntentKind, local_commit_log::NewLocalCommitLog, remote_commit_log::CommitResult,
    ConnectionExt, Store, XmtpOpenMlsProvider,
};

/// This trait wraps openmls' merge_staged_commit function to include
/// commit logging to help in fork resolution.
pub trait MergeStagedCommitAndLog {
    fn merge_staged_commit_and_log<Db: ConnectionExt>(
        &mut self,
        provider: &XmtpOpenMlsProvider<Db>,
        staged_commit: StagedCommit,
        validated_commit: &ValidatedCommit,
        sequence_id: i64,
    ) -> Result<(), GroupMessageProcessingError>;
}

impl MergeStagedCommitAndLog for MlsGroup {
    fn merge_staged_commit_and_log<Db: ConnectionExt>(
        &mut self,
        provider: &XmtpOpenMlsProvider<Db>,
        staged_commit: StagedCommit,
        validated_commit: &ValidatedCommit,
        sequence_id: i64,
    ) -> Result<(), GroupMessageProcessingError> {
        let last_epoch_authenticator = self.epoch_authenticator().as_slice().to_vec();
        self.merge_staged_commit(&provider, staged_commit)?;

        // Reuse intent kind here to represent the commit type, even if it's an external commit
        // This is for debugging purposes only, so an approximation is fine
        let metadata_info = &validated_commit.metadata_validation_info;
        let commit_type = if !validated_commit.added_inboxes.is_empty()
            || !validated_commit.removed_inboxes.is_empty()
            || validated_commit.installations_changed
        {
            IntentKind::UpdateGroupMembership
        } else if validated_commit.permissions_changed {
            IntentKind::UpdatePermission
        } else if !metadata_info.admins_added.is_empty()
            || !metadata_info.admins_removed.is_empty()
            || !metadata_info.super_admins_added.is_empty()
            || !metadata_info.super_admins_removed.is_empty()
        {
            IntentKind::UpdateAdminList
        } else if !metadata_info.metadata_field_changes.is_empty() {
            IntentKind::MetadataUpdate
        } else {
            IntentKind::KeyUpdate
        };

        NewLocalCommitLog {
            group_id: self.group_id().to_vec(),
            commit_sequence_id: sequence_id,
            last_epoch_authenticator,
            commit_result: CommitResult::Success,
            applied_epoch_number: Some(self.epoch().as_u64() as i64),
            applied_epoch_authenticator: Some(self.epoch_authenticator().as_slice().to_vec()),
            sender_inbox_id: Some(validated_commit.actor_inbox_id()),
            sender_installation_id: Some(validated_commit.actor_installation_id()),
            commit_type: Some(format!("{}", commit_type)),
            error_message: None,
        }
        .store(provider.db())?;

        Ok(())
    }
}
