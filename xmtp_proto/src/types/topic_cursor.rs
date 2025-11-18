use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use crate::types::{Cursor, GlobalCursor, Topic};

/// A cursor that keeps a [`super::GlobalCursor`] for each topic it has seen.
#[derive(Default, Debug, PartialEq, Clone)]
pub struct TopicCursor {
    inner: HashMap<Topic, GlobalCursor>,
}

impl TopicCursor {
    /// get the item at [`Topic`] or insert the default
    pub fn get_or_default(&mut self, topic: &Topic) -> &GlobalCursor {
        self.inner.entry(topic.clone()).or_default()
    }
}

impl Deref for TopicCursor {
    type Target = HashMap<Topic, GlobalCursor>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for TopicCursor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl TopicCursor {
    pub fn add(&mut self, topic: Topic, cursor: GlobalCursor) {
        self.inner.insert(topic, cursor);
    }

    /// apply [`Cursor`] to this topic cursor
    pub fn apply(&mut self, topic: Topic, cursor: &Cursor) {
        let v = self.inner.entry(topic).or_default();
        v.apply(cursor);
    }
}

impl FromIterator<(Topic, GlobalCursor)> for TopicCursor {
    fn from_iter<T: IntoIterator<Item = (Topic, GlobalCursor)>>(iter: T) -> Self {
        TopicCursor {
            inner: HashMap::from_iter(iter),
        }
    }
}

impl IntoIterator for TopicCursor {
    type Item = (Topic, GlobalCursor);
    type IntoIter = <HashMap<Topic, GlobalCursor> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a TopicCursor {
    type Item = (&'a Topic, &'a GlobalCursor);
    type IntoIter = std::collections::hash_map::Iter<'a, Topic, GlobalCursor>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<'a> IntoIterator for &'a mut TopicCursor {
    type Item = (&'a Topic, &'a mut GlobalCursor);
    type IntoIter = std::collections::hash_map::IterMut<'a, Topic, GlobalCursor>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter_mut()
    }
}
