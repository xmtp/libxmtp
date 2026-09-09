//! Position on one backend topic.
use serde::{Deserialize, Serialize};

use super::SequenceId;
use crate::backend_v1;

/// Highest sequence id processed on one topic. Zero starts at the beginning.
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Cursor(pub SequenceId);

impl From<backend_v1::Cursor> for Cursor {
    fn from(value: backend_v1::Cursor) -> Self {
        Self(value.sequence_id)
    }
}

impl From<Cursor> for backend_v1::Cursor {
    fn from(value: Cursor) -> Self {
        Self {
            sequence_id: value.0,
        }
    }
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl xmtp_common::Generate for Cursor {
    fn generate() -> Self {
        Self(xmtp_common::rand_u64())
    }
}
