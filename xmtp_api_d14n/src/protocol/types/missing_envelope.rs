use xmtp_proto::types::{Cursor, Topic};

/// An envelope that is depended on by another envelope,
/// but is missing from our local database or from a network call
/// see [`ResolveDependencies`](crate::protocol::ResolveDependencies) and
/// [`Sort`](crate::protocol::Sort)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MissingEnvelope {
    pub topic: Topic,
    pub cursor: Cursor,
}

impl MissingEnvelope {
    pub fn new(topic: Topic, cursor: Cursor) -> Self {
        Self { topic, cursor }
    }
}

impl std::fmt::Display for MissingEnvelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.cursor, self.topic)
    }
}
