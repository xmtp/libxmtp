//! Types representing the current representation of the world to libxmtp
mod api_identifier;
mod app_version;
mod cursor;
mod global_cursor;
mod group_message;
mod ids;
mod message_metadata;
mod topic;
mod topic_cursor;
mod welcome_message;
pub use api_identifier::*;
pub use app_version::*;
pub use cursor::*;
pub use global_cursor::*;
pub use group_message::*;
pub use ids::*;
pub use message_metadata::*;
pub use topic::*;
pub use topic_cursor::*;
pub use welcome_message::*;

pub type OriginatorId = u32;
pub type SequenceId = u64;

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(TopicKind::GroupMessagesV1, 0)]
    #[case(TopicKind::WelcomeMessagesV1, 1)]
    #[case(TopicKind::IdentityUpdatesV1, 2)]
    #[case(TopicKind::KeyPackagesV1, 3)]
    fn test_topic_kind_values(#[case] topic_kind: TopicKind, #[case] expected_value: u8) {
        assert_eq!(topic_kind as u8, expected_value);
    }
}
