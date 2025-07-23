use crate::groups::GroupError;
use crate::groups::{mls_sync::GroupMessageProcessingError, validated_commit::ValidatedCommit};
use crate::identity::Identity;
use openmls::group::{MlsGroup, MlsGroupCreateConfig, StagedCommit};
use openmls::prelude::CredentialWithKey;
use openmls::prelude::GroupId;
use openmls::prelude::StagedWelcome;
use xmtp_db::{
    local_commit_log::{CommitType, NewLocalCommitLog},
    remote_commit_log::CommitResult,
    ConnectionExt, Store, XmtpOpenMlsProvider,
};

/// This trait wraps openmls groups to include commit logs for any mutations to encryption state.
/// This helps with fork detection.
pub trait CommitLogStorer: std::marker::Sized {
    fn from_creation_logged<Db: ConnectionExt>(
        provider: &XmtpOpenMlsProvider<Db>,
        identity: &Identity,
        group_config: &MlsGroupCreateConfig,
    ) -> Result<Self, GroupError>;

    fn from_backup_stub_logged<Db: ConnectionExt>(
        provider: &XmtpOpenMlsProvider<Db>,
        identity: &Identity,
        group_config: &MlsGroupCreateConfig,
        group_id: GroupId,
    ) -> Result<Self, GroupError>;

    fn from_welcome_logged<Db: ConnectionExt>(
        provider: &XmtpOpenMlsProvider<Db>,
        welcome: StagedWelcome,
        sender_inbox_id: &str,
        sender_installation_id: &[u8],
    ) -> Result<Self, GroupError>;

    fn merge_staged_commit_logged<Db: ConnectionExt>(
        &mut self,
        provider: &XmtpOpenMlsProvider<Db>,
        staged_commit: StagedCommit,
        validated_commit: &ValidatedCommit,
        sequence_id: i64,
    ) -> Result<(), GroupMessageProcessingError>;
}

impl CommitLogStorer for MlsGroup {
    fn from_creation_logged<Db: ConnectionExt>(
        provider: &XmtpOpenMlsProvider<Db>,
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

        if crate::configuration::ENABLE_COMMIT_LOG {
            NewLocalCommitLog {
                group_id: mls_group.group_id().to_vec(),
                commit_sequence_id: 0,
                last_epoch_authenticator: vec![],
                commit_result: CommitResult::Success,
                applied_epoch_number: mls_group.epoch().as_u64() as i64,
                applied_epoch_authenticator: mls_group.epoch_authenticator().as_slice().to_vec(),
                sender_inbox_id: Some(identity.inbox_id().to_string()),
                sender_installation_id: Some(identity.installation_id().to_vec()),
                commit_type: Some(format!("{}", CommitType::GroupCreation)),
                error_message: None,
            }
            .store(provider.db())?;
        }

        Ok(mls_group)
    }

    fn from_backup_stub_logged<Db: ConnectionExt>(
        provider: &XmtpOpenMlsProvider<Db>,
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

        if crate::configuration::ENABLE_COMMIT_LOG {
            // It is safe to log this stubbed encryption state, because we will not upload anything
            // to the remote commit log with a sequence ID of 0.
            NewLocalCommitLog {
                group_id: mls_group.group_id().to_vec(),
                commit_sequence_id: 0,
                last_epoch_authenticator: vec![],
                commit_result: CommitResult::Success,
                applied_epoch_number: mls_group.epoch().as_u64() as i64,
                applied_epoch_authenticator: mls_group.epoch_authenticator().as_slice().to_vec(),
                sender_inbox_id: None,
                sender_installation_id: None,
                commit_type: Some(format!("{}", CommitType::BackupRestore)),
                error_message: None,
            }
            .store(provider.db())?;
        }

        Ok(mls_group)
    }

    fn from_welcome_logged<Db: ConnectionExt>(
        provider: &XmtpOpenMlsProvider<Db>,
        welcome: StagedWelcome,
        sender_inbox_id: &str,
        sender_installation_id: &[u8],
    ) -> Result<Self, GroupError> {
        let mls_group = welcome.into_group(provider)?;

        if crate::configuration::ENABLE_COMMIT_LOG {
            NewLocalCommitLog {
                group_id: mls_group.group_id().to_vec(),
                // TODO(rich): Replace with the cursor sequence ID of the welcome once implemented
                commit_sequence_id: 0,
                last_epoch_authenticator: vec![],
                commit_result: CommitResult::Success,
                applied_epoch_number: mls_group.epoch().as_u64() as i64,
                applied_epoch_authenticator: mls_group.epoch_authenticator().as_slice().to_vec(),
                sender_inbox_id: Some(sender_inbox_id.to_string()),
                sender_installation_id: Some(sender_installation_id.to_vec()),
                commit_type: Some(format!("{}", CommitType::Welcome)),
                error_message: None,
            }
            .store(provider.db())?;
        }

        Ok(mls_group)
    }

    fn merge_staged_commit_logged<Db: ConnectionExt>(
        &mut self,
        provider: &XmtpOpenMlsProvider<Db>,
        staged_commit: StagedCommit,
        validated_commit: &ValidatedCommit,
        sequence_id: i64,
    ) -> Result<(), GroupMessageProcessingError> {
        let last_epoch_authenticator = self.epoch_authenticator().as_slice().to_vec();
        self.merge_staged_commit(&provider, staged_commit)?;

        if crate::configuration::ENABLE_COMMIT_LOG {
            NewLocalCommitLog {
                group_id: self.group_id().to_vec(),
                commit_sequence_id: sequence_id,
                last_epoch_authenticator,
                commit_result: CommitResult::Success,
                applied_epoch_number: self.epoch().as_u64() as i64,
                applied_epoch_authenticator: self.epoch_authenticator().as_slice().to_vec(),
                sender_inbox_id: Some(validated_commit.actor_inbox_id()),
                sender_installation_id: Some(validated_commit.actor_installation_id()),
                commit_type: Some(format!("{}", validated_commit.debug_commit_type())),
                error_message: None,
            }
            .store(provider.db())?;
        }

        Ok(())
    }
}
