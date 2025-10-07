/// Constant Originator IDs for v3 compatibility
pub struct Originators;

impl Originators {
    pub const MLS_COMMITS: u16 = 0;
    pub const INBOX_LOG: u16 = 1;

    pub const APPLICATION_MESSAGES: u16 = 10;
    pub const WELCOME_MESSAGES: u16 = 11;
    /// Key Packages
    pub const INSTALLATIONS: u16 = 13;
    pub const REMOTE_COMMIT_LOG: u16 = 100;
}
