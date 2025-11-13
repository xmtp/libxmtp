use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use crate::types::{GlobalCursor, Topic};

/// A cursor that keeps a [`super::GlobalCursor`] for each topic it has seen.
#[derive(Default, PartialEq)]
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
}
