use openmls::group::{MlsGroup, StagedCommit};
use xmtp_db::{
    group_intent::IntentState, local_commit_log::LocalCommitLog, remote_commit_log::CommitResult,
    ConnectionExt, Store, XmtpOpenMlsProvider,
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
    ) -> Result<IntentState, Result<IntentState, GroupMessageProcessingError>>;
}

impl MergeStagedCommitAndLog for MlsGroup {
    fn merge_staged_commit_and_log<Db: ConnectionExt>(
        &mut self,
        provider: &XmtpOpenMlsProvider<Db>,
        staged_commit: StagedCommit,
        validated_commit: &ValidatedCommit,
    ) -> Result<IntentState, Result<IntentState, GroupMessageProcessingError>> {
        let mut log = LocalCommitLog {
            epoch_authenticator: self.epoch_authenticator().as_slice().to_vec(),
            epoch_number: Some(self.epoch().as_u64() as i64),
            group_id: Some(self.group_id().to_vec()),
            result: CommitResult::Success,
            sender_inbox_id: validated_commit.actor_inbox_id(),
            sender_installation_id: validated_commit.actor_installation_id(),
        };

        let store = |log: LocalCommitLog| {
            log.store(provider.db())
                .map_err(|e| Err(GroupMessageProcessingError::Storage(e)))
        };

        if let Err(err) = self.merge_staged_commit(&provider, staged_commit) {
            tracing::error!("error merging commit: {err}");
            log.result = CommitResult::Invalid;
            store(log)?;
            return Err(Ok(IntentState::ToPublish));
        }

        store(log)?;

        Ok(IntentState::Committed)
    }
}
