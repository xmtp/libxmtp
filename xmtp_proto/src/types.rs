//! Types representing the current representation of the world to libxmtp
mod group_message;
mod welcome_message;
mod cursor;
mod ids;
pub use ids::*;
pub use cursor::*;
pub use welcome_message::*;
pub use group_message::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TopicKind {
    GroupMessagesV1 = 0,
    WelcomeMessagesV1,
    IdentityUpdatesV1,
    KeyPackagesV1,
}
