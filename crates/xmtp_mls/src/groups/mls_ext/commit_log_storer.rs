use crate::StorageError;
use crate::groups::GroupError;
use crate::groups::{mls_sync::GroupMessageProcessingError, validated_commit::ValidatedCommit};
use crate::identity::Identity;
use openmls::group::{MlsGroup, MlsGroupCreateConfig, StagedCommit};
use openmls::prelude::CredentialWithKey;
use openmls::prelude::GroupEpoch;
use openmls::prelude::GroupId;
use openmls::prelude::StagedWelcome;
use xmtp_db::MlsProviderExt;
#[cfg(feature = "commit-log")]
use xmtp_db::{
    Store, XmtpMlsStorageProvider,
    local_commit_log::{CommitType, NewLocalCommitLog},
    prelude::*,
    remote_commit_log::CommitResult,
};

/// This trait wraps openmls groups to include commit logs for any mutations to encryption state.
/// This helps with fork detection.
pub trait CommitLogStorer: std::marker::Sized {
    fn from_creation_logged(
        provider: &impl MlsProviderExt,
        identity: &Identity,
        group_config: &MlsGroupCreateConfig,
    ) -> Result<Self, GroupError>;

    fn from_backup_stub_logged(
        provider: &impl MlsProviderExt,
        identity: &Identity,
        group_config: &MlsGroupCreateConfig,
        group_id: GroupId,
    ) -> Result<Self, GroupError>;

    fn from_welcome_logged(
        provider: &impl MlsProviderExt,
        welcome: StagedWelcome,
        sender_inbox_id: &str,
        sender_installation_id: &[u8],
    ) -> Result<Self, GroupError>;

    fn merge_staged_commit_logged(
        &mut self,
        provider: &impl MlsProviderExt,
        staged_commit: StagedCommit,
        validated_commit: &ValidatedCommit,
        sequence_id: i64,
    ) -> Result<(), GroupMessageProcessingError>;

    /// Marks a commit as failed in the commit log.
    /// Only call this when the status of the commit is final.
    /// Specifically, do not call this for retryable errors, or
    /// VersionTooLow/GroupPaused errors.
    fn mark_failed_commit_logged(
        &self,
        provider: &impl MlsProviderExt,
        commit_cursor: u64,
        commit_epoch: GroupEpoch,
        error: &GroupMessageProcessingError,
    ) -> Result<(), StorageError>;
}

impl CommitLogStorer for MlsGroup {
    fn from_creation_logged(
        provider: &impl MlsProviderExt,
        identity: &Identity,
        group_config: &MlsGroupCreateConfig,
    ) -> Result<Self, GroupError> {
        let mls_group = MlsGroup::new(
            provider,
            &identity.installation_keys,
            group_config,
            CredentialWithKey {
                credential: identity.credential(),
                signature_key: identity.installation_keys.public_slice().into(),
            },
        )?;

        #[cfg(feature = "commit-log")]
        NewLocalCommitLog {
            group_id: mls_group.group_id().to_vec(),
            commit_sequence_id: 0,
            commit_result: CommitResult::Success,
            applied_epoch_number: mls_group.epoch().as_u64() as i64,
            applied_epoch_authenticator: mls_group.epoch_authenticator().as_slice().to_vec(),
            sender_inbox_id: Some(identity.inbox_id().to_string()),
            sender_installation_id: Some(identity.installation_id().to_vec()),
            commit_type: Some(format!("{}", CommitType::GroupCreation)),
            error_message: None,
        }
        .store(&provider.key_store().db())?;

        Ok(mls_group)
    }

    fn from_backup_stub_logged(
        provider: &impl MlsProviderExt,
        identity: &Identity,
        group_config: &MlsGroupCreateConfig,
        group_id: GroupId,
    ) -> Result<Self, GroupError> {
        let mls_group = MlsGroup::new_with_group_id(
            provider,
            &identity.installation_keys,
            group_config,
            group_id,
            CredentialWithKey {
                credential: identity.credential(),
                signature_key: identity.installation_keys.public_slice().into(),
            },
        )?;

        // It is safe to log this stubbed encryption state, because we will not upload anything
        // to the remote commit log with a sequence ID of 0.
        #[cfg(feature = "commit-log")]
        NewLocalCommitLog {
            group_id: mls_group.group_id().to_vec(),
            commit_sequence_id: 0,
            commit_result: CommitResult::Success,
            applied_epoch_number: mls_group.epoch().as_u64() as i64,
            applied_epoch_authenticator: mls_group.epoch_authenticator().as_slice().to_vec(),
            sender_inbox_id: None,
            sender_installation_id: None,
            commit_type: Some(format!("{}", CommitType::BackupRestore)),
            error_message: None,
        }
        .store(&provider.key_store().db())?;

        Ok(mls_group)
    }

    fn from_welcome_logged(
        provider: &impl MlsProviderExt,
        welcome: StagedWelcome,
        _sender_inbox_id: &str,
        _sender_installation_id: &[u8],
    ) -> Result<Self, GroupError> {
        // Failed welcomes do not need to be added to the commit log
        let mls_group = welcome.into_group(provider)?;

        #[cfg(feature = "commit-log")]
        NewLocalCommitLog {
            group_id: mls_group.group_id().to_vec(),
            // TODO(rich): Replace with the cursor sequence ID of the welcome once implemented
            commit_sequence_id: 0,
            commit_result: CommitResult::Success,
            applied_epoch_number: mls_group.epoch().as_u64() as i64,
            applied_epoch_authenticator: mls_group.epoch_authenticator().as_slice().to_vec(),
            sender_inbox_id: Some(_sender_inbox_id.to_string()),
            sender_installation_id: Some(_sender_installation_id.to_vec()),
            commit_type: Some(format!("{}", CommitType::Welcome)),
            error_message: None,
        }
        .store(&provider.key_store().db())?;

        Ok(mls_group)
    }

    fn merge_staged_commit_logged(
        &mut self,
        provider: &impl MlsProviderExt,
        staged_commit: StagedCommit,
        _validated_commit: &ValidatedCommit,
        _sequence_id: i64,
    ) -> Result<(), GroupMessageProcessingError> {
        self.merge_staged_commit(provider, staged_commit)?;

        #[cfg(feature = "commit-log")]
        {
            NewLocalCommitLog {
                group_id: self.group_id().to_vec(),
                commit_sequence_id: _sequence_id,
                commit_result: CommitResult::Success,
                applied_epoch_number: self.epoch().as_u64() as i64,
                applied_epoch_authenticator: self.epoch_authenticator().as_slice().to_vec(),
                sender_inbox_id: Some(_validated_commit.actor_inbox_id()),
                sender_installation_id: Some(_validated_commit.actor_installation_id()),
                commit_type: Some(format!("{}", _validated_commit.debug_commit_type())),
                error_message: None,
            }
            .store(&provider.key_store().db())?;
        }

        Ok(())
    }

    #[cfg(not(feature = "commit-log"))]
    fn mark_failed_commit_logged(
        &self,
        _provider: &impl MlsProviderExt,
        _commit_sequence_id: u64,
        _commit_epoch: GroupEpoch,
        _error: &GroupMessageProcessingError,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    #[cfg(feature = "commit-log")]
    fn mark_failed_commit_logged(
        &self,
        provider: &impl MlsProviderExt,
        commit_sequence_id: u64,
        commit_epoch: GroupEpoch,
        error: &GroupMessageProcessingError,
    ) -> Result<(), StorageError> {
        let group_id = self.group_id().to_vec();
        let last_epoch_number = self.epoch();
        let last_epoch_authenticator = self.epoch_authenticator();
        let conn = provider.key_store().db();
        let mut maybe_recently_welcomed = true;
        // Latest log may not exist if a client upgraded from a version without local commit logs
        if let Some(latest_log) = conn.get_latest_log_for_group(&group_id)?
            && latest_log.commit_type != Some(CommitType::Welcome.to_string())
        {
            maybe_recently_welcomed = false;
        }
        // If we've recently joined the group, we may get a bunch of wrong epoch errors
        // until we 'catch up' to the commit that spawned the welcome. We can ignore these for now.
        if commit_epoch.as_u64() <= last_epoch_number.as_u64() && maybe_recently_welcomed {
            return Ok(());
        }

        NewLocalCommitLog {
            group_id: group_id.to_vec(),
            commit_sequence_id: commit_sequence_id as i64,
            commit_result: error.commit_result(),
            applied_epoch_number: last_epoch_number.as_u64() as i64,
            applied_epoch_authenticator: last_epoch_authenticator.as_slice().to_vec(),
            error_message: Some(format!("{:?}", error)),
            sender_inbox_id: None,
            sender_installation_id: None,
            commit_type: None,
        }
        .store(&conn)?;
        Ok(())
    }
}
