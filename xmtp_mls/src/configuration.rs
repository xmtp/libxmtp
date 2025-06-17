use openmls::versions::ProtocolVersion;

use xmtp_common::{NS_IN_30_DAYS, NS_IN_HOUR, NS_IN_SEC};
pub use xmtp_cryptography::configuration::{CIPHERSUITE, POST_QUANTUM_CIPHERSUITE};

pub struct DeviceSyncUrls;

impl DeviceSyncUrls {
    pub const LOCAL_ADDRESS: &'static str = "http://0.0.0.0:5558";
    pub const DEV_ADDRESS: &'static str = "https://message-history.dev.ephemera.network";
    pub const PRODUCTION_ADDRESS: &'static str = "https://message-history.ephemera.network";
}

/// Duration to wait before restarting workers in case of an error.
pub const WORKER_RESTART_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

pub const MLS_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::Mls10;

pub const WELCOME_HPKE_LABEL: &str = "MLS_WELCOME";

pub const MAX_GROUP_SYNC_RETRIES: usize = 3;

pub const MAX_INTENT_PUBLISH_ATTEMPTS: usize = 3;

pub const GROUP_KEY_ROTATION_INTERVAL_NS: i64 = NS_IN_30_DAYS;

#[allow(dead_code)]
const SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS: i64 = NS_IN_HOUR / 2; // 30 min

pub const SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS: i64 = 5 * NS_IN_SEC;

pub const MAX_GROUP_SIZE: usize = 220;

pub const MAX_PAST_EPOCHS: usize = 3;

/// the max amount of data that can be sent in one gRPC call
/// we leave 5 * 1024 * 1024 as extra buffer room
pub const GRPC_DATA_LIMIT: usize = 45 * 1024 * 1024;

pub const CREATE_PQ_KEY_PACKAGE_EXTENSION: bool = true;

// If a metadata field name starts with this character,
// and it does not have a policy set, it is a super admin only field
pub const SUPER_ADMIN_METADATA_PREFIX: &str = "_";
pub(crate) const HMAC_SALT: &[u8] = b"libXMTP HKDF salt!";

#[cfg(any(test, feature = "test-utils"))]
pub mod debug_config {
    use super::*;
    pub(crate) const SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS: i64 = NS_IN_HOUR / 3600;
    // 1 second
}

pub fn sync_update_installations_interval_ns() -> i64 {
    #[cfg(any(test, feature = "test-utils"))]
    {
        debug_config::SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS
    }
    #[cfg(not(any(test, feature = "test-utils")))]
    {
        SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS
    }
}
