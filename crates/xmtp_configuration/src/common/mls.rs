//! Common configuration values between dev & prod
use openmls::versions::ProtocolVersion;

use xmtp_common::{NS_IN_30_DAYS, NS_IN_SEC};
pub use xmtp_cryptography::configuration::{CIPHERSUITE, POST_QUANTUM_CIPHERSUITE};

/// Duration to wait before restarting workers in case of an error.
pub const WORKER_RESTART_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

pub const MLS_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::Mls10;

pub const WELCOME_HPKE_LABEL: &str = "MLS_WELCOME";

/// HPKE domain-separation label for external-invite GroupInfo payloads
/// wrapped via [`payload_encryption::wrap_payload_hpke`]. Distinct from
/// [`WELCOME_HPKE_LABEL`] to prevent cross-protocol oracle attacks.
///
/// The v1 external-invite flow uses symmetric AEAD encryption only (see
/// `xmtp_mls_common::invite::encrypted_group_info`), but the label is
/// reserved here so a future HPKE-based external-invite path can adopt it
/// without churning the public API.
///
/// [`payload_encryption::wrap_payload_hpke`]: https://docs.rs/xmtp_mls_common/latest/xmtp_mls_common/mls_ext/payload_encryption/fn.wrap_payload_hpke.html
pub const XMTP_EXTERNAL_INVITE_LABEL: &str = "XMTP_EXTERNAL_INVITE";

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

/// Default floor written into `MIN_SUPPORTED_PROTOCOL_VERSION` when a
/// group is migrated via `enable_proposals` without an explicit override.
///
/// Set to the release where the AppData-migration / proposals feature
/// first ships. Clients older than this version cannot read the
/// AppData dictionary, so the welcome-time / commit-time pause path
/// uses this value to gate them out of migrated groups before they
/// fork.
///
/// **Invariant**: must be `<= CARGO_PKG_VERSION`. The send-side clamp
/// in `enable_proposals` refuses any `min_version > own pkg_version`
/// (you can't legally write a floor you yourself don't satisfy), so
/// a constant ahead of the workspace version would brick every
/// production call to `enable_proposals` that takes the default. Bump
/// this in lockstep with the workspace version, never independently.
///
/// Callers that need a different floor (testing, dev nightlies,
/// staged rollouts) pass `EnableProposalsOptions::min_version` instead
/// of relying on this default.
pub const PROPOSALS_MIN_PROTOCOL_VERSION: &str = "1.11.0-dev";

// Welcome pointers are mostly the hpke public key and less than 100 bytes for the welcome pointer
// so as long as we have 2 installations that need a single welcome it will result in less data being
// ingested by the nodes and stored. There is a slight penalty for egress data, but the amount needed
// to be stored can be 100x less than using regular welcome messages.
pub const INSTALLATION_THRESHOLD_FOR_WELCOME_POINTER_SENDING: usize = 2;

/// the base backoff time that is multiplied by 3
pub const SYNC_BACKOFF_WAIT_MS: u16 = 50;
/// the total wait for all attempts
pub const SYNC_BACKOFF_TOTAL_WAIT_MAX_SECS: u16 = 10;
/// jitter time between attempts in ms
pub const SYNC_JITTER_MS: u16 = 25;
