//! Common configuration values between dev & prod
use openmls::versions::ProtocolVersion;

use xmtp_common::{NS_IN_30_DAYS, NS_IN_SEC};
pub use xmtp_cryptography::configuration::{CIPHERSUITE, POST_QUANTUM_CIPHERSUITE};

/// Duration to wait before restarting workers in case of an error.
pub const WORKER_RESTART_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

pub const MLS_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::Mls10;

pub const WELCOME_HPKE_LABEL: &str = "MLS_WELCOME";

pub const MAX_GROUP_SYNC_RETRIES: usize = 3;

pub const MAX_INTENT_PUBLISH_ATTEMPTS: usize = 3;

pub const GROUP_KEY_ROTATION_INTERVAL_NS: i64 = NS_IN_30_DAYS;

pub const KEY_PACKAGE_QUEUE_INTERVAL_NS: i64 = 5 * NS_IN_SEC; // 5 secs

/// Interval in NS used to compute `next_key_package_rotation_ns`.
/// This defines how often a new KeyPackage should be *rotated*,
/// but does *not* determine the actual KeyPackage expiration.
pub const KEY_PACKAGE_ROTATION_INTERVAL_NS: i64 = NS_IN_30_DAYS; // 30 days

pub const SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS: i64 = 5 * NS_IN_SEC;

pub const MAX_GROUP_SIZE: usize = 250;

pub const MAX_INSTALLATIONS_PER_INBOX: usize = 10;

pub const MAX_PAST_EPOCHS: usize = 3;

pub const CREATE_PQ_KEY_PACKAGE_EXTENSION: bool = true;

// If a metadata field name starts with this character,
// and it does not have a policy set, it is a super admin only field
pub const SUPER_ADMIN_METADATA_PREFIX: &str = "_";
pub const HMAC_SALT: &[u8] = b"libXMTP HKDF salt!";

pub const ENABLE_COMMIT_LOG: bool = true;
pub const MIN_RECOVERY_REQUEST_VERSION: &str = "1.6.0";
