

/// Constant Originator IDs for v3 compatibility
pub struct Originators;

impl Originators {
    pub const MLS_COMMITS: u16 = 0;
    pub const INBOX_LOG: u16 = 1;

    pub const APPLICATION_MESSAGE: u16 = 10;
    pub const WELCOME_MESSAGES: u16 = 11;
    pub const INSTALLATIONS: u16 = 13;
}
