use super::*;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) enum DependencyKey {
    Identity(IdentityRequirement),
    Welcome(Cursor),
    GroupPrefix(GroupId, Cursor),
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) enum DependencyParent {
    GroupHead(Topic, Cursor),
    IdentityHead(Topic, Cursor),
    Welcome(Cursor),
}

#[derive(Default)]
struct Dependency {
    parents: HashSet<DependencyParent>,
    running: bool,
}

/// Shared requests own their exact parents. Prefix watches use no request permit.
#[derive(Default)]
pub(super) struct DependencyRegistry {
    entries: HashMap<DependencyKey, Dependency>,
}

impl DependencyRegistry {
    pub(super) fn attach(&mut self, parent: DependencyParent, key: DependencyKey) {
        self.detach(&parent);
        self.entries.entry(key).or_default().parents.insert(parent);
    }

    pub(super) fn detach(&mut self, parent: &DependencyParent) {
        self.retain_parents(|candidate| candidate != parent);
    }

    pub(super) fn retain_parents(&mut self, mut keep: impl FnMut(&DependencyParent) -> bool) {
        self.entries.retain(|_, dependency| {
            dependency.parents.retain(&mut keep);
            dependency.running || !dependency.parents.is_empty()
        });
    }

    pub(super) fn contains(&self, parent: &DependencyParent) -> bool {
        self.entries
            .values()
            .any(|entry| entry.parents.contains(parent))
    }

    pub(super) fn start_queued(&mut self, available: usize) -> Vec<DependencyKey> {
        self.entries
            .iter_mut()
            .filter(|(key, entry)| {
                !matches!(key, DependencyKey::GroupPrefix(..))
                    && !entry.running
                    && !entry.parents.is_empty()
            })
            .take(available)
            .map(|(key, entry)| {
                entry.running = true;
                key.clone()
            })
            .collect()
    }

    pub(super) fn finish(&mut self, key: &DependencyKey) -> HashSet<DependencyParent> {
        self.entries
            .remove(key)
            .map(|entry| entry.parents)
            .unwrap_or_default()
    }

    pub(super) fn prefixes(&self) -> impl Iterator<Item = (Cursor, GroupId, Cursor)> + '_ {
        self.entries.iter().flat_map(|(key, entry)| {
            entry
                .parents
                .iter()
                .filter_map(move |parent| match (key, parent) {
                    (
                        DependencyKey::GroupPrefix(group, anchor),
                        DependencyParent::Welcome(cursor),
                    ) => Some((*cursor, *group, *anchor)),
                    _ => None,
                })
        })
    }
}
