use xmtp_common::NS_IN_HOUR;

/// Constant Originator IDs for v3 compatibility
pub struct Originators;

impl Originators {
    pub const MLS_COMMITS: u32 = 0;
    pub const INBOX_LOG: u32 = 1;

    pub const APPLICATION_MESSAGES: u32 = 10;
    pub const WELCOME_MESSAGES: u32 = 11;
    /// Key Packages
    pub const INSTALLATIONS: u32 = 13;
    pub const REMOTE_COMMIT_LOG: u32 = 100;
    /// the "default" originator for local and tests
    pub const DEFAULT: u32 = 100;
}

pub const PAYER_WRITE_FILTER: &str = "xmtp.xmtpv4.payer_api.PayerApi";

/// How often to refresh the cutover time
/// Set to 6 hours.
pub const CUTOVER_REFRESH_TIME: i64 = NS_IN_HOUR * 6;

pub const D14N_MIGRATION_MSG_REGEX: &str = r#"(publishing to XMTP V3 is no longer available|XMTP V3 streaming is no longer available)\. Please upgrade your client to XMTP D14N\."#;
