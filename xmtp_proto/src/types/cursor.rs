//! xmtp message cursor type and implementations
use serde::{Deserialize, Serialize};
use std::iter::Once;
use std::{collections::HashMap, fmt};
use xmtp_configuration::Originators;

use crate::types::{OriginatorId, SequenceId};
use crate::xmtp::xmtpv4;

/// XMTP cursor type
/// represents a position in an ordered sequence of messages, belonging
// force use of the `new` constructor w/ non_exhaustive
/// so we retain some control of the internal structure/use of this type
/// and disallow ad-hoc construction
/// while still allowing access to fields with `.field` notation
///
/// _NOTE_ the `Ordering` implementation does not have the of a vector clock.
/// for instance, an ordering for (sid:oid) pairs (10:0).cmp(11:10)
/// will return "Less Than". This does not indicate a relationship for a [`VectorClock`],
/// rather indicates a relationship between the rust primitives on the type.
/// for `VectorClock` ordering relationships, use the [`VectorClock`](crate::traits::VectorClock) trait
/// on a vector clock compatible type.
/// The ordering on this type may be used for efficient lookups in
/// a [`BTreeMap`](std::collections::BTreeMap) or [`Vec`] separate from VectorClocks.
/// for instance [`CursorList`](super::CursorList) keeps a sorted list of cursors
#[non_exhaustive]
#[derive(
    Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Cursor {
    pub sequence_id: super::SequenceId,
    pub originator_id: super::OriginatorId,
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
            originator_id: Originators::REMOTE_COMMIT_LOG,
        }
    }

    pub const fn v3_welcomes(sequence_id: u64) -> Self {
        Self {
            sequence_id,
            originator_id: Originators::WELCOME_MESSAGES,
        }
    }

    pub const fn v3_messages(sequence_id: u64) -> Self {
        Self {
            sequence_id,
            originator_id: Originators::APPLICATION_MESSAGES,
        }
    }

    pub const fn installations(sequence_id: u64) -> Self {
        Self {
            sequence_id,
            originator_id: Originators::INSTALLATIONS,
        }
    }

    pub const fn mls_commits(sequence_id: u64) -> Self {
        Self {
            sequence_id,
            originator_id: Originators::MLS_COMMITS,
        }
    }

    pub const fn inbox_log(sequence_id: u64) -> Self {
        Self {
            sequence_id,
            originator_id: Originators::INBOX_LOG,
        }
    }
}

impl fmt::Display for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[sid({:6}):oid({:3})]",
            self.sequence_id, self.originator_id
        )
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

impl From<Cursor> for xmtpv4::envelopes::Cursor {
    fn from(value: Cursor) -> Self {
        let mut map = HashMap::new();
        map.insert(value.originator_id, value.sequence_id);
        xmtpv4::envelopes::Cursor {
            node_id_to_sequence_id: map,
        }
    }
}

impl<'a> IntoIterator for &'a Cursor {
    type Item = (&'a OriginatorId, &'a SequenceId);
    type IntoIter = Once<(&'a OriginatorId, &'a SequenceId)>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once((&self.originator_id, &self.sequence_id))
    }
}

impl<'a> IntoIterator for &'a mut Cursor {
    type Item = (&'a mut OriginatorId, &'a mut SequenceId);
    type IntoIter = Once<(&'a mut OriginatorId, &'a mut SequenceId)>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once((&mut self.originator_id, &mut self.sequence_id))
    }
}

impl IntoIterator for Cursor {
    type Item = (OriginatorId, SequenceId);
    type IntoIter = Once<(OriginatorId, SequenceId)>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once((self.originator_id, self.sequence_id))
    }
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(Cursor::commit_log(100), 100, Originators::REMOTE_COMMIT_LOG)]
    #[case(Cursor::v3_welcomes(200), 200, Originators::WELCOME_MESSAGES)]
    #[case(Cursor::v3_messages(300), 300, Originators::APPLICATION_MESSAGES)]
    #[case(Cursor::installations(400), 400, Originators::INSTALLATIONS)]
    #[case(Cursor::mls_commits(500), 500, Originators::MLS_COMMITS)]
    #[case(Cursor::inbox_log(600), 600, Originators::INBOX_LOG)]
    fn test_originator_constructors(
        #[case] cursor: Cursor,
        #[case] expected_seq: u64,
        #[case] expected_orig: u32,
    ) {
        assert_eq!(cursor.sequence_id, expected_seq);
        assert_eq!(cursor.originator_id, expected_orig);
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
}
