//! Commit log worker test steps reachable from other crates.

use super::*;

// Test related types
/// The commit-log worker steps that `CommitLogWorker::run_test` runs.
#[cfg(any(test, feature = "test-utils"))]
#[doc(hidden)]
pub enum CommitLogTestFunction {
    PublishCommitLogsToRemote,
    SaveRemoteCommitLog,
    CheckForkedState,
    All,
}

/// What each step of one `CommitLogWorker::run_test` pass returned.
#[cfg(any(test, feature = "test-utils"))]
#[doc(hidden)]
pub struct TestResult {
    pub save_remote_commit_log_results: Option<HashMap<Vec<u8>, usize>>,
    pub publish_commit_log_results: Option<Vec<ConversationCursorInfo>>,
    pub is_forked: Option<HashMap<Vec<u8>, Option<bool>>>,
}

// Helper that exposes private methods for testing
#[cfg(any(test, feature = "test-utils"))]
impl<Context> CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    #[cfg(test)]
    pub async fn _tick(&mut self) -> Result<(), CommitLogError> {
        self.tick().await
    }

    /// The worker's loop, as the supervisor runs it.
    #[cfg(test)]
    pub async fn _run(&mut self) -> Result<(), CommitLogError> {
        self.run().await
    }

    /// Pass every seal result through `fault`.
    #[cfg(test)]
    pub(crate) fn with_seal_fault(mut self, fault: SealFault) -> Self {
        self.seal_fault = Some(fault);
        self
    }

    // Test helper to get fork status for all groups that would be checked (for backward compatibility with tests)
    /// The stored fork status of every group.
    #[doc(hidden)]
    pub fn get_all_fork_statuses(&self) -> Result<HashMap<Vec<u8>, Option<bool>>, CommitLogError> {
        use xmtp_db::group::GroupQueryArgs;
        let conn = &self.context.db();
        // Get all groups (not just those with commit log keys)
        let all_groups = conn.find_groups(GroupQueryArgs::default())?;

        let mut results = HashMap::new();
        for group in all_groups {
            let fork_status = conn.get_group_commit_log_forked_status(&group.id)?;
            results.insert(group.id.to_vec(), fork_status);
        }

        Ok(results)
    }

    /// Test-only version that runs without infinite loop
    #[doc(hidden)]
    pub async fn run_test(
        &mut self,
        commit_log_test_function: CommitLogTestFunction,
        iterations: Option<usize>,
    ) -> Result<Vec<TestResult>, CommitLogError> {
        let mut test_results = Vec::new();
        match iterations {
            Some(n) => {
                // Run exactly n times
                for _ in 0..n {
                    let test_result = self.test_helper(&commit_log_test_function).await?;
                    test_results.push(test_result);
                }
            }
            None => {
                let test_result = self.test_helper(&commit_log_test_function).await?;
                test_results.push(test_result);
            }
        }
        Ok(test_results)
    }

    async fn test_helper(
        &mut self,
        commit_log_test_function: &CommitLogTestFunction,
    ) -> Result<TestResult, CommitLogError> {
        let mut test_result = TestResult {
            save_remote_commit_log_results: None,
            publish_commit_log_results: None,
            is_forked: None,
        };
        match commit_log_test_function {
            CommitLogTestFunction::PublishCommitLogsToRemote => {
                let publish_commit_log_results = self.publish_commit_logs_to_remote().await?;
                test_result.publish_commit_log_results = Some(publish_commit_log_results);
            }
            CommitLogTestFunction::SaveRemoteCommitLog => {
                let save_remote_commit_log_results = self.save_remote_commit_log().await?;
                test_result.save_remote_commit_log_results = Some(save_remote_commit_log_results);
            }
            CommitLogTestFunction::CheckForkedState => {
                self.update_forked_state().await?;
                let is_forked = self.get_all_fork_statuses()?;
                test_result.is_forked = Some(is_forked);
            }
            CommitLogTestFunction::All => {
                // Order is save; update fork status; publish
                let save_remote_commit_log_results = self.save_remote_commit_log().await?;
                test_result.save_remote_commit_log_results = Some(save_remote_commit_log_results);
                self.update_forked_state().await?;
                let is_forked = self.get_all_fork_statuses()?;
                test_result.is_forked = Some(is_forked);
                let publish_commit_log_results = self.publish_commit_logs_to_remote().await?;
                test_result.publish_commit_log_results = Some(publish_commit_log_results);
            }
        }
        Ok(test_result)
    }
}
