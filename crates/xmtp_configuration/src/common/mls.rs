//! Common configuration values between dev & prod
use crate::ns::{NS_IN_30_DAYS, NS_IN_SEC};

/// Duration to wait before restarting workers in case of an error.
pub const WORKER_RESTART_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

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

/// Most actions one identity update may carry. Validators reject a larger
/// update when they decode it, before they verify any of its signatures, and
/// clients refuse to build one. It is also the most installations a client
/// accepts as a deployment's installation ceiling, so one update can revoke
/// every installation that ceiling allows.
pub const MAX_IDENTITY_UPDATE_ACTIONS: usize = 100;

pub const MAX_PAST_EPOCHS: usize = 3;

/// Largest gap from a sender's ratchet head accepted in the current epoch.
/// OpenMLS persists this configuration per group; changing it needs a migration.
pub const SENDER_RATCHET_MAX_FORWARD_DISTANCE: u32 = 1_000;

/// Number of earlier sender generations retained for out-of-order messages.
pub const SENDER_RATCHET_OUT_OF_ORDER_TOLERANCE: u32 = 5;

/// Healthy primary-read time allowed for an absent pinned identity update.
/// This exceeds the backend's default statement timeout.
pub const IDENTITY_REFERENCE_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// First interval between primary reads for absent identity updates. Each
/// later interval doubles up to `IDENTITY_REFERENCE_MAX_RETRY_INTERVAL`.
pub const IDENTITY_REFERENCE_RETRY_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(250);

/// Longest interval between primary reads for absent identity updates.
pub const IDENTITY_REFERENCE_MAX_RETRY_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(2);

/// Maximum concurrent identity proof requests in one resolver batch, and the
/// most association states one batch read derives at once.
pub const IDENTITY_DEPENDENCY_CONCURRENCY: usize = 8;

pub const CREATE_PQ_KEY_PACKAGE_EXTENSION: bool = true;

// If a metadata field name starts with this character,
// and it does not have a policy set, it is a super admin only field
pub const SUPER_ADMIN_METADATA_PREFIX: &str = "_";
pub const HMAC_SALT: &[u8] = b"libXMTP HKDF salt!";

pub const ENABLE_COMMIT_LOG: bool = true;
pub const MIN_RECOVERY_REQUEST_VERSION: &str = "1.6.0";

/// The protocol floor written when a group or DM is created.
///
/// Its runtime effect is nil by construction: this floor must be at or below
/// `CARGO_PKG_VERSION`. The version gate becomes load-bearing when a later
/// floor is raised above a shipped client version. Keep the gate wired so
/// those clients pause and resume after an upgrade.
///
/// Follow the floor-bump convention in the component registry when changing
/// the wire format. Raise this constant with the workspace version.
pub const PROPOSALS_MIN_PROTOCOL_VERSION: &str = "1.12.0-dev.1";

/// First version whose receivers accept an inbox adding back its own leafless
/// installation. Senders retry only in groups whose floor is at least this.
pub const INSTALLATION_RETRY_MIN_PROTOCOL_VERSION: &str = "1.12.0-dev.1";

// Welcome pointers are mostly the hpke public key and less than 100 bytes for the welcome pointer
// so as long as we have 2 installations that need a single welcome it will result in less data being
// ingested by the nodes and stored. There is a slight penalty for egress data, but the amount needed
// to be stored can be 100x less than using regular welcome messages.
pub const INSTALLATION_THRESHOLD_FOR_WELCOME_POINTER_SENDING: usize = 2;

/// Length of a welcome pointer's ChaCha20-Poly1305 pointee key.
pub const WELCOME_POINTEE_KEY_BYTES: usize = 32;

/// Length of each of a welcome pointer's two ChaCha20-Poly1305 nonces.
pub const WELCOME_POINTEE_NONCE_BYTES: usize = 12;

/// Length of a key package's Curve25519 Welcome wrapper key.
pub const WELCOME_WRAPPER_CURVE25519_KEY_BYTES: usize = 32;

/// Length of a key package's X-Wing (ML-KEM-768 draft 6) Welcome wrapper key.
pub const WELCOME_WRAPPER_XWING_KEY_BYTES: usize = 1216;

/// the base backoff time that is multiplied by 3
pub const SYNC_BACKOFF_WAIT_MS: u16 = 50;
/// the total wait for all attempts
pub const SYNC_BACKOFF_TOTAL_WAIT_MAX_SECS: u16 = 10;
/// jitter time between attempts in ms
pub const SYNC_JITTER_MS: u16 = 25;

/// How long after the backend's timestamp on a Welcome the client keeps it
/// waiting for a welcome pointee or for its group's message topic to reach the
/// join anchor.
pub const WELCOME_RETENTION_NS: i64 = 3 * crate::ns::NS_IN_DAY;
