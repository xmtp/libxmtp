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
impl xmtp_common::Generate for Cursor {
    fn generate() -> Self {
        Cursor {
            sequence_id: xmtp_common::rand_u64(),
            originator_id: openmls::test_utils::random_u32(),
        }
    }
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(Cursor::commit_log(100), 100, Originators::REMOTE_COMMIT_LOG as u32)]
    #[case(Cursor::welcomes(200), 200, Originators::WELCOME_MESSAGES as u32)]
    #[case(Cursor::v3_messages(300), 300, Originators::APPLICATION_MESSAGES as u32)]
    #[case(Cursor::installations(400), 400, Originators::INSTALLATIONS as u32)]
    #[case(Cursor::mls_commits(500), 500, Originators::MLS_COMMITS as u32)]
    #[case(Cursor::inbox_log(600), 600, Originators::INBOX_LOG as u32)]
    fn test_originator_constructors(
        #[case] cursor: Cursor,
        #[case] expected_seq: u64,
        #[case] expected_orig: u32,
    ) {
        assert_eq!(cursor.sequence_id, expected_seq);
        assert_eq!(cursor.originator_id, expected_orig);
    }

    #[rstest]
    #[case(Cursor::new(100, 1u32), "[sid[100]:oid[1]]")]
    #[case(Cursor::new(0, 0u32), "[sid[0]:oid[0]]")]
    fn test_display(#[case] cursor: Cursor, #[case] expected: &str) {
        assert_eq!(format!("{}", cursor), expected);
    }

    #[rstest]
    #[case(Cursor::new(1, 1u32), Cursor::new(2, 1u32), true)] // same originator, different seq
    #[case(Cursor::new(2, 1u32), Cursor::new(1, 1u32), false)]
    #[case(Cursor::new(1, 1u32), Cursor::new(1, 1u32), false)] // equal
    #[case(Cursor::new(1, 1u32), Cursor::new(1, 2u32), true)] // different originators
    fn test_ordering(#[case] cursor1: Cursor, #[case] cursor2: Cursor, #[case] cursor1_less: bool) {
        assert_eq!(cursor1 < cursor2, cursor1_less);
        assert_eq!(cursor1 == cursor2, !cursor1_less && cursor2 >= cursor1);
    }

    #[xmtp_common::test]
    fn test_display_max_values() {
        assert_eq!(
            format!("{}", Cursor::new(u64::MAX, u32::MAX)),
            format!("[sid[{}]:oid[{}]]", u64::MAX, u32::MAX)
        );
    }
}
