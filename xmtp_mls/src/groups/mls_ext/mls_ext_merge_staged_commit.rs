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
        let mut log = NewLocalCommitLog {
            group_id: self.group_id().to_vec(),
            commit_sequence_id: sequence_id,
            last_epoch_authenticator: self.epoch_authenticator().as_slice().to_vec(),
            commit_result: CommitResult::Unknown,
            applied_epoch_number: None,
            applied_epoch_authenticator: None,
            sender_inbox_id: Some(validated_commit.actor_inbox_id()),
            sender_installation_id: Some(validated_commit.actor_installation_id()),
            commit_type: None,
        };

        if let Err(err) = self.merge_staged_commit(&provider, staged_commit) {
            tracing::error!("Error merging commit: {err}");
            log.commit_result = CommitResult::Invalid;
            log.store(provider.db())?;
            return Err(err)?;
        }

        // TODO: Fill in fields from commit_result down
        log.store(provider.db())?;
        Ok(())
    }
}
