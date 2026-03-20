use xmtp_common::RetryableError;

#[derive(Clone, Debug)]
pub enum NetworkConsistencyQuorum {
    AllNodes,
    Majority,
    Count(u32),
}

impl NetworkConsistencyQuorum {
    pub fn required(&self, total: usize) -> usize {
        match self {
            Self::AllNodes => total,
            Self::Majority => total / 2 + 1,
            Self::Count(n) => (*n as usize).max(1).min(total),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NetworkConsistencyOpts {
    pub quorum: NetworkConsistencyQuorum,
    /// Maximum number of poll attempts per node before giving up.
    pub max_attempts: u32,
    /// Starting backoff delay in milliseconds between retries.
    pub initial_delay_ms: u64,
    /// Maximum backoff delay in milliseconds (cap for exponential backoff).
    pub max_delay_ms: u64,
    /// Overall wall-clock timeout in milliseconds; whichever fires first
    /// (timeout_ms or max_attempts exhaustion) terminates the poll.
    pub timeout_ms: u64,
}

impl Default for NetworkConsistencyOpts {
    fn default() -> Self {
        Self {
            quorum: NetworkConsistencyQuorum::AllNodes,
            max_attempts: 10,
            initial_delay_ms: 100,
            max_delay_ms: 2_000,
            timeout_ms: 30_000,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkConsistencyError {
    #[error("Quorum not reached: {confirmed}/{required} nodes confirmed within timeout")]
    QuorumNotReached { confirmed: usize, required: usize },
    #[error("Node discovery failed: {0}")]
    NodeDiscovery(String),
    #[error("Consistency check timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },
}

impl RetryableError for NetworkConsistencyError {
    fn is_retryable(&self) -> bool {
        false
    }
}

#[xmtp_common::async_trait]
pub trait NetworkConsistencyProvider: Send + Sync {
    async fn wait_until_visible(
        &self,
        topics: xmtp_proto::types::TopicCursor,
        opts: &NetworkConsistencyOpts,
    ) -> Result<(), NetworkConsistencyError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test]
    fn quorum_required_all_nodes() {
        assert_eq!(NetworkConsistencyQuorum::AllNodes.required(3), 3);
        assert_eq!(NetworkConsistencyQuorum::AllNodes.required(0), 0);
    }

    #[xmtp_common::test]
    fn quorum_required_majority() {
        assert_eq!(NetworkConsistencyQuorum::Majority.required(3), 2);
        assert_eq!(NetworkConsistencyQuorum::Majority.required(4), 3);
        assert_eq!(NetworkConsistencyQuorum::Majority.required(1), 1);
    }

    #[xmtp_common::test]
    fn quorum_required_count_capped_at_total() {
        assert_eq!(NetworkConsistencyQuorum::Count(5).required(3), 3);
        assert_eq!(NetworkConsistencyQuorum::Count(2).required(3), 2);
        assert_eq!(NetworkConsistencyQuorum::Count(0).required(3), 1);
    }

    #[xmtp_common::test]
    fn default_opts() {
        let opts = NetworkConsistencyOpts::default();
        assert_eq!(opts.max_attempts, 10);
        assert_eq!(opts.initial_delay_ms, 100);
        assert_eq!(opts.max_delay_ms, 2_000);
        assert_eq!(opts.timeout_ms, 30_000);
        assert!(matches!(opts.quorum, NetworkConsistencyQuorum::AllNodes));
    }
}
