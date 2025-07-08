use crate::groups::{mls_sync::GroupMessageProcessingError, validated_commit::ValidatedCommit};
use openmls::group::{MlsGroup, StagedCommit};
use xmtp_db::{
    local_commit_log::NewLocalCommitLog, remote_commit_log::CommitResult, ConnectionExt, Store,
    XmtpOpenMlsProvider,
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

        Ok(())
    }
}
