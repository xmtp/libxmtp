//! A list of cursors
//! this is mostly for readable the Display implementation

use std::{collections::BTreeSet, ops::Deref};

use crate::types::Cursor;

/// A owned, sorted list of [`Cursor`]'s that may be
/// used for efficient lookups/storage of [`Cursor`]
/// does not contain duplicates.
#[derive(Debug, Clone, Default)]
pub struct CursorList {
    inner: BTreeSet<Cursor>,
}

impl CursorList {
    pub fn new() -> Self {
        Self {
            inner: BTreeSet::new(),
        }
    }

    pub fn with_vec(cursors: Vec<Cursor>) -> Self {
        Self {
            inner: BTreeSet::from_iter(cursors),
        }
    }
}

impl From<Vec<Cursor>> for CursorList {
    fn from(value: Vec<Cursor>) -> CursorList {
        Self::with_vec(value)
    }
}

impl Deref for CursorList {
    type Target = BTreeSet<Cursor>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::fmt::Display for CursorList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for c in &self.inner {
            write!(f, "{}", c)?;
        }
        Ok(())
    }
}

impl FromIterator<Cursor> for CursorList {
    fn from_iter<T: IntoIterator<Item = Cursor>>(iter: T) -> Self {
        CursorList::with_vec(Vec::from_iter(iter))
    }
}
