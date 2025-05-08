use openmls::versions::ProtocolVersion;

pub use xmtp_cryptography::configuration::CIPHERSUITE;

pub struct DeviceSyncUrls;

impl DeviceSyncUrls {
    pub const LOCAL_ADDRESS: &'static str = "http://0.0.0.0:5558";
    pub const DEV_ADDRESS: &'static str = "https://message-history.dev.ephemera.network/";
    pub const PRODUCTION_ADDRESS: &'static str = "https://message-history.ephemera.network/";
}

/// Duration to wait before restarting workers in case of an error.
pub const WORKER_RESTART_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

pub const MLS_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::Mls10;

pub const WELCOME_HPKE_LABEL: &str = "MLS_WELCOME";

pub const MAX_GROUP_SYNC_RETRIES: usize = 3;

pub const MAX_INTENT_PUBLISH_ATTEMPTS: usize = 3;

const NS_IN_SEC: i64 = 1_000_000_000;

pub const NS_IN_HOUR: i64 = NS_IN_SEC * 60 * 60;

const NS_IN_DAY: i64 = NS_IN_HOUR * 24;

pub const GROUP_KEY_ROTATION_INTERVAL_NS: i64 = 30 * NS_IN_DAY;

#[allow(dead_code)]
const SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS: i64 = NS_IN_HOUR / 2; // 30 min

pub const SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS: i64 = 5 * NS_IN_SEC;

pub const MAX_GROUP_SIZE: usize = 200;

pub const MAX_PAST_EPOCHS: usize = 3;

/// the max amount of data that can be sent in one gRPC call
/// we leave 5 * 1024 * 1024 as extra buffer room
pub const GRPC_DATA_LIMIT: usize = 45 * 1024 * 1024;

/// MLS Extension Types
///
/// Copied from draft-ietf-mls-protocol-16:
///
/// | Value            | Name                     | Message(s) | Recommended | Reference |
/// |:-----------------|:-------------------------|:-----------|:------------|:----------|
/// | 0x0000           | RESERVED                 | N/A        | N/A         | RFC XXXX  |
/// | 0x0001           | application_id           | LN         | Y           | RFC XXXX  |
/// | 0x0002           | ratchet_tree             | GI         | Y           | RFC XXXX  |
/// | 0x0003           | required_capabilities    | GC         | Y           | RFC XXXX  |
/// | 0x0004           | external_pub             | GI         | Y           | RFC XXXX  |
/// | 0x0005           | external_senders         | GC         | Y           | RFC XXXX  |
/// | 0xff00  - 0xffff | Reserved for Private Use | N/A        | N/A         | RFC XXXX  |
pub const MUTABLE_METADATA_EXTENSION_ID: u16 = 0xff00;
pub const GROUP_MEMBERSHIP_EXTENSION_ID: u16 = 0xff01;
pub const GROUP_PERMISSIONS_EXTENSION_ID: u16 = 0xff02;

pub const DEFAULT_GROUP_NAME: &str = "";
pub const DEFAULT_GROUP_DESCRIPTION: &str = "";
pub const DEFAULT_GROUP_IMAGE_URL_SQUARE: &str = "";

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
