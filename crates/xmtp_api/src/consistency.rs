// Re-export consistency types from xmtp_api_d14n where they are defined.
// They live in xmtp_api_d14n to avoid a circular dependency
// (xmtp_api depends on xmtp_api_d14n).
pub use xmtp_api_d14n::consistency::{
    NetworkConsistencyError, NetworkConsistencyOpts, NetworkConsistencyProvider,
    NetworkConsistencyQuorum,
};

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
        // Count(0) means zero nodes required — quorum always satisfied
        assert_eq!(NetworkConsistencyQuorum::Count(0).required(3), 0);
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
