//! xmtp message cursor type and implementations
use serde::{Deserialize, Serialize};
use std::fmt;

/// XMTP cursor type
/// represents a position in an ordered sequence of messages, belonging
#[derive(
    Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
// TODO:d14n comparing cursors is unsafe/undefined behavior if originator ids are not equal.
// maybe Ord should not be derived?
pub struct Cursor {
    pub originator_id: u32,
    pub sequence_id: u64,
}

impl Cursor {
    pub fn new<O: Into<u32>>(sequence_id: u64, originator_id: O) -> Self {
        Self {
            sequence_id,
            originator_id: originator_id.into(),
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
