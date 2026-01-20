//! A list of cursors
//! this is mostly for readable the Display implementation

use std::ops::Deref;

use crate::types::Cursor;

/// A owned list of [`Cursor`]
#[derive(Debug, Clone)]
pub struct CursorList {
    inner: Vec<Cursor>,
}

impl CursorList {
    pub fn new(cursors: Vec<Cursor>) -> Self {
        Self { inner: cursors }
    }
}

impl From<Vec<Cursor>> for CursorList {
    fn from(value: Vec<Cursor>) -> CursorList {
        CursorList { inner: value }
    }
}

impl Deref for CursorList {
    type Target = [Cursor];

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
        CursorList::new(Vec::from_iter(iter))
    }
}
