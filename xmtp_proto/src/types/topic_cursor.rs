use std::{
    collections::{HashMap, hash_map::Entry},
    ops::{Deref, DerefMut},
};

use crate::{
    api::VectorClock,
    types::{GlobalCursor, InstallationId, Topic},
};

/// A cursor that keeps a [`super::GlobalCursor`] for each topic it has seen.
#[derive(Default, Debug, PartialEq, Clone)]
pub struct TopicCursor {
    inner: HashMap<Topic, GlobalCursor>,
}

pub type TopicEntry<'a> = Entry<'a, Topic, GlobalCursor>;

impl TopicCursor {
    /// get the item at [`Topic`] or insert the default
    pub fn get_or_default(&mut self, topic: &Topic) -> &GlobalCursor {
        self.inner.entry(topic.clone()).or_default()
    }

    /// get the [`GlobalCursor`] corresponding to the [`super::TopicKind::GroupMessagesV1`]
    /// by [`super::GroupId`]
    pub fn get_group(&self, group_id: impl AsRef<[u8]>) -> GlobalCursor {
        self.inner
            .get(&Topic::new_group_message(group_id))
            .cloned()
            .unwrap_or_default()
    }

    /// check if this topic cursor contains [`super::GroupId`]
    pub fn contains_group(&self, group_id: impl AsRef<[u8]>) -> bool {
        self.inner.contains_key(&Topic::new_group_message(group_id))
    }

    /// Computes the Lowest Common Cursor (LCC) across all topics.
    ///
    /// For each originator node, takes the minimum sequence ID seen across
    /// all topics. Returns an empty cursor if this TopicCursor is empty.
    pub fn lcc(&self) -> GlobalCursor {
        self.values().fold(GlobalCursor::default(), |mut acc, c| {
            acc.merge_least(c);
            acc
        })
    }

    /// Computes the Greatest Common Cursor (GCC) across all topics.
    ///
    /// For each originator node, takes the maximum sequence ID seen across
    /// all topics. Returns an empty cursor if this TopicCursor is empty.
    pub fn gcc(&self) -> GlobalCursor {
        self.values().fold(GlobalCursor::default(), |mut acc, c| {
            acc.merge(c);
            acc
        })
    }

    /// consume this topic cursor into a list of its topics
    pub fn into_topics(self) -> Vec<Topic> {
        self.inner.into_keys().collect()
    }

    /// get a [`Vec`] of all [`Topic`] in this cursor
    /// by cloning.
    pub fn topics(&self) -> Vec<Topic> {
        self.inner.keys().cloned().collect()
    }

    /// entry api for only [super::TopicKind::GroupMessagesV1]
    pub fn group_entry(&mut self, group_id: impl AsRef<[u8]>) -> TopicEntry<'_> {
        self.inner.entry(Topic::new_group_message(group_id))
    }

    /// entry api for only [super::TopicKind::IdentityUpdatesV1]
    pub fn identity_entry(&mut self, inbox_id: impl AsRef<[u8]>) -> TopicEntry<'_> {
        self.inner.entry(Topic::new_identity_update(inbox_id))
    }

    /// entry api for only [super::TopicKind::WelcomeMessagesV1]
    pub fn welcome_entry(&mut self, installation_id: InstallationId) -> TopicEntry<'_> {
        self.inner
            .entry(Topic::new_welcome_message(installation_id))
    }

    /// entry api for only [super::TopicKind::KeyPackagesV1]
    pub fn key_package_entry(&mut self, installation_id: InstallationId) -> TopicEntry<'_> {
        self.inner.entry(Topic::new_key_package(installation_id))
    }

    /// iterate over all [`super::TopicKind::GroupMessagesV1`]
    /// topics as a pair of (&[`super::GroupId`], &[`GlobalCursor`])
    pub fn groups(&self) -> impl Iterator<Item = (&[u8], &GlobalCursor)> {
        self.inner
            .iter()
            .filter_map(|(t, g)| Some((t.group_message_v1()?.identifier(), g)))
    }
}

impl From<HashMap<Topic, GlobalCursor>> for TopicCursor {
    fn from(inner: HashMap<Topic, GlobalCursor>) -> Self {
        TopicCursor { inner }
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

impl std::fmt::Display for TopicCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (topic, has_seen) in self.inner.iter() {
            writeln!(f, "{} -> {}", topic, has_seen)?;
        }
        Ok(())
    }
}
