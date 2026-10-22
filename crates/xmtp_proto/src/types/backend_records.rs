use crate::{
    backend_v1::{EnvelopeMeta, SealedCommitLogEntry},
    types::{Cursor, Topic},
    xmtp::identity::associations::IdentityUpdate,
};

/// One envelope read from a commit-log topic, not yet opened.
#[derive(Clone, Debug)]
pub struct CommitLogEntry {
    /// The commit-log topic the envelope was read from.
    pub topic: Topic,
    /// The envelope's position on that topic.
    pub cursor: Cursor,
    /// The sealed entry, or `None` when the envelope carries another
    /// payload. A reader discards such an envelope and reads past it.
    pub sealed: Option<SealedCommitLogEntry>,
}
#[derive(Clone, Debug)]
pub struct IdentityUpdateLog {
    pub meta: EnvelopeMeta,
    pub update: IdentityUpdate,
}
