use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Default, Debug)]
pub struct ApiStats {
    pub upload_key_package: Arc<EndpointStats>,
    pub fetch_key_package: Arc<EndpointStats>,
    pub send_group_messages: Arc<EndpointStats>,
    pub send_welcome_messages: Arc<EndpointStats>,
    pub query_group_messages: Arc<EndpointStats>,
    pub query_welcome_messages: Arc<EndpointStats>,
    pub subscribe_messages: Arc<EndpointStats>,
    pub subscribe_welcomes: Arc<EndpointStats>,
    pub publish_commit_log: Arc<EndpointStats>,
    pub query_commit_log: Arc<EndpointStats>,
}

impl ApiStats {
    pub fn clear(&self) {
        self.upload_key_package.clear();
        self.fetch_key_package.clear();
        self.send_group_messages.clear();
        self.send_welcome_messages.clear();
        self.query_group_messages.clear();
        self.query_welcome_messages.clear();
        self.subscribe_messages.clear();
        self.subscribe_welcomes.clear();
        self.publish_commit_log.clear();
        self.query_commit_log.clear();
    }
}

#[derive(Clone, Default, Debug)]
pub struct IdentityStats {
    pub publish_identity_update: Arc<EndpointStats>,
    pub get_identity_updates_v2: Arc<EndpointStats>,
    pub get_inbox_ids: Arc<EndpointStats>,
    pub verify_smart_contract_wallet_signature: Arc<EndpointStats>,
}

impl IdentityStats {
    pub fn clear(&self) {
        self.publish_identity_update.clear();
        self.get_identity_updates_v2.clear();
        self.get_inbox_ids.clear();
        self.verify_smart_contract_wallet_signature.clear();
    }
}

pub struct AggregateStats {
    pub mls: ApiStats,
    pub identity: IdentityStats,
}

#[derive(Default, Debug)]
pub struct EndpointStats {
    request_count: AtomicUsize,
}

impl std::fmt::Display for EndpointStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.request_count.load(Ordering::Relaxed))
    }
}

impl EndpointStats {
    pub fn count_request(&self) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_count(&self) -> usize {
        self.request_count.load(Ordering::Relaxed)
    }
    pub fn clear(&self) {
        self.request_count.store(0, Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test]
    fn test_endpoint_stats_clear() {
        let stats = EndpointStats::default();

        // Add some counts
        stats.count_request();
        stats.count_request();
        assert_eq!(stats.get_count(), 2);

        // Clear should reset to 0
        stats.clear();
        assert_eq!(stats.get_count(), 0);

        // Should be able to count again after clear
        stats.count_request();
        assert_eq!(stats.get_count(), 1);
    }

    #[xmtp_common::test]
    fn test_endpoint_stats_display() {
        let stats = EndpointStats::default();

        // Display shows current count
        assert_eq!(format!("{}", stats), "0");

        stats.count_request();
        stats.count_request();
        assert_eq!(format!("{}", stats), "2");

        stats.clear();
        assert_eq!(format!("{}", stats), "0");
    }
}
