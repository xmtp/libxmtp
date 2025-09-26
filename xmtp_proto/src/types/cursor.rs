//! xmtp message cursor type and implementations
use serde::{Deserialize, Serialize};
use std::fmt;
use xmtp_configuration::Originators;

/// XMTP cursor type
/// represents a position in an ordered sequence of messages, belonging
#[derive(
    Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
// TODO:d14n comparing cursors is unsafe/undefined behavior if originator ids are not equal.
// maybe Ord should not be derived?
pub struct Cursor {
    pub originator_id: super::OriginatorId,
    pub sequence_id: super::SequenceId,
}

impl Cursor {
    pub fn new<O: Into<u32>>(sequence_id: u64, originator_id: O) -> Self {
        Self {
            sequence_id,
            originator_id: originator_id.into(),
        }
    }

    pub const fn commit_log(sequence_id: u64) -> Self {
        Self {
            sequence_id,
            originator_id: Originators::REMOTE_COMMIT_LOG as u32,
        }
    }

    pub const fn welcomes(sequence_id: u64) -> Self {
        Self {
            sequence_id,
            originator_id: Originators::WELCOME_MESSAGES as u32,
        }
    }

    pub const fn v3_messages(sequence_id: u64) -> Self {
        Self {
            sequence_id,
            originator_id: Originators::APPLICATION_MESSAGES as u32,
        }
    }

    pub const fn installations(sequence_id: u64) -> Self {
        Self {
            sequence_id,
            originator_id: Originators::INSTALLATIONS as u32,
        }
    }

    pub const fn mls_commits(sequence_id: u64) -> Self {
        Self {
            sequence_id,
            originator_id: Originators::MLS_COMMITS as u32,
        }
    }

    pub const fn inbox_log(sequence_id: u64) -> Self {
        Self {
            sequence_id,
            originator_id: Originators::INBOX_LOG as u32,
        }
    }
}

impl fmt::Display for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[sid[{}]:oid[{}]]", self.sequence_id, self.originator_id)
    }
}

#[cfg(any(test, feature = "test-utils"))]
mod test {
    use openmls::test_utils::random_u32;
    use xmtp_common::{rand_u64, Generate};

    use super::*;

    impl Generate for Cursor {
        fn generate() -> Self {
            Cursor {
                sequence_id: rand_u64(),
                originator_id: random_u32(),
            }
        }
    }
}
