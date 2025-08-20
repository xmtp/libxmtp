//! Types representing the current representation of the world to libxmtp
mod cursor;
mod group_message;
mod ids;
mod welcome_message;
pub use cursor::*;
pub use group_message::*;
pub use ids::*;
pub use welcome_message::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TopicKind {
    GroupMessagesV1 = 0,
    WelcomeMessagesV1,
    IdentityUpdatesV1,
    KeyPackagesV1,
}
