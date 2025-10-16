//! Test-Specific configuration values

use xmtp_common::NS_IN_SEC;

pub const SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS: i64 = NS_IN_SEC; // 1 Second

pub const KEYS_EXPIRATION_INTERVAL_NS: i64 = 3 * NS_IN_SEC; //3 seconds

pub const ENABLE_COMMIT_LOG: bool = true;
pub const ENABLE_RECOVERY_REQUESTS: bool = true;
