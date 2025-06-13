use openmls::group::{MlsGroup, StagedCommit};
use xmtp_common::time::now_ns;
use xmtp_db::{
    local_commit_log::NewLocalCommitLog, remote_commit_log::CommitResult, ConnectionExt, Store,
    XmtpOpenMlsProvider,
};

use crate::groups::{mls_sync::GroupMessageProcessingError, validated_commit::ValidatedCommit};

/// This trait wraps openmls' merge_staged_commit function to include
/// commit logging to help in fork resolution.
pub trait MergeStagedCommitAndLog {
    fn merge_staged_commit_and_log<Db: ConnectionExt>(
        &mut self,
        provider: &XmtpOpenMlsProvider<Db>,
        staged_commit: StagedCommit,
        validated_commit: &ValidatedCommit,
        sequence_id: Option<i64>,
    ) -> Result<(), GroupMessageProcessingError>;
}

impl MergeStagedCommitAndLog for MlsGroup {
    fn merge_staged_commit_and_log<Db: ConnectionExt>(
        &mut self,
        provider: &XmtpOpenMlsProvider<Db>,
        staged_commit: StagedCommit,
        validated_commit: &ValidatedCommit,
        sequence_id: Option<i64>,
    ) -> Result<(), GroupMessageProcessingError> {
        let mut log = NewLocalCommitLog {
            epoch_authenticator: Some(self.epoch_authenticator().as_slice().to_vec()),
            sequence_id,
            epoch_number: Some(self.epoch().as_u64() as i64),
            group_id: self.group_id().to_vec(),
            result: CommitResult::Success,
            sender_inbox_id: Some(validated_commit.actor_inbox_id()),
            sender_installation_id: Some(validated_commit.actor_installation_id()),
        };

        if let Err(err) = self.merge_staged_commit(&provider, staged_commit) {
            tracing::error!("Error merging commit: {err}");
            log.result = CommitResult::Invalid;
            log.store(provider.db())?;
            return Err(err)?;
        }

        log.store(provider.db())?;
        Ok(())
    }
}
