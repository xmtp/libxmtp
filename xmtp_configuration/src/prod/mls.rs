use xmtp_common::{NS_IN_DAY, NS_IN_HOUR};

pub const SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS: i64 = NS_IN_HOUR / 2; // 30 min

pub const KEYS_EXPIRATION_INTERVAL_NS: i64 = NS_IN_DAY; // 1 day
