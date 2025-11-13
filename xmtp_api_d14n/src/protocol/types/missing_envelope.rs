use xmtp_proto::types::{Cursor, Topic};

/// An envelope that is depended on by another envelope,
/// but is missing from our local database or from a network call
/// see [`ResolveDependencies`](crate::protocol::ResolveDependencies) and
/// [`Sort`](crate::protocol::Sort)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MissingEnvelope {
    topic: Topic,
    cursor: Cursor,
}

impl MissingEnvelope {
    pub fn new(topic: Topic, cursor: Cursor) -> Self {
        Self { topic, cursor }
    }
}
