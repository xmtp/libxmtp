use xmtp_proto::types::{Cursor, Topic};

/// An envelope that is depended on by another envelope,
/// but is so far missing from the local database or the network
/// see [`ResolveDependencies`](crate::protocol::ResolveDependencies) and
/// [`Sort`](crate::protocol::Sort)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RequiredDependency {
    pub needed_by: Cursor,
    pub topic: Topic,
    pub cursor: Cursor,
}

impl RequiredDependency {
    pub fn new(topic: Topic, cursor: Cursor, needed_by: Cursor) -> Self {
        Self {
            topic,
            cursor,
            needed_by,
        }
    }
}

impl std::fmt::Display for RequiredDependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.cursor, self.topic)
    }
}
