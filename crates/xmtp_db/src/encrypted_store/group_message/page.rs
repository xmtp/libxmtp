use super::*;
use crate::StorageError;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

const BODY_QUERY_BATCH: usize = 500;

#[cfg(test)]
thread_local! {
    pub(super) static BEFORE_BODIES: std::cell::RefCell<Option<Box<dyn FnMut()>>> = const { std::cell::RefCell::new(None) };
}

struct Head<K> {
    key: K,
    source: usize,
    ascending: bool,
}

impl<K: Eq> PartialEq for Head<K> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl<K: Eq> Eq for Head<K> {}

impl<K: Ord> PartialOrd for Head<K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<K: Ord> Ord for Head<K> {
    fn cmp(&self, other: &Self) -> Ordering {
        let order = self.key.cmp(&other.key);
        if self.ascending {
            order.reverse()
        } else {
            order
        }
    }
}

/// Merge bounded source keys. Remove the extra key before any body is read.
pub(super) fn select_keys<K: Ord>(
    mut sources: Vec<VecDeque<K>>,
    limit: i64,
    ascending: bool,
) -> (Vec<K>, bool) {
    let mut heap = BinaryHeap::new();
    for (source, rows) in sources.iter_mut().enumerate() {
        if let Some(key) = rows.pop_front() {
            heap.push(Head {
                key,
                source,
                ascending,
            });
        }
    }
    let mut selected = Vec::new();
    while (selected.len() as i64) <= limit {
        let Some(head) = heap.pop() else { break };
        selected.push(head.key);
        if let Some(key) = sources[head.source].pop_front() {
            heap.push(Head {
                key,
                source: head.source,
                ascending,
            });
        }
    }
    let has_more = selected.len() as i64 > limit;
    if has_more {
        selected.pop();
    }
    (selected, has_more)
}

type Bodies = HashMap<Vec<u8>, (StoredGroupMessage, Option<i64>)>;

pub(super) fn load_bodies(
    conn: &mut diesel::SqliteConnection,
    ids: &[&[u8]],
) -> Result<Bodies, StorageError> {
    #[cfg(test)]
    BEFORE_BODIES.with_borrow_mut(|hook| {
        if let Some(hook) = hook.as_mut() {
            hook();
        }
    });
    let mut stored = HashMap::new();
    for batch in ids.chunks(BODY_QUERY_BATCH) {
        for (message, delivery_sequence) in dsl::group_messages
            .filter(dsl::id.eq_any(batch))
            .select((StoredGroupMessage::as_select(), dsl::delivery_sequence))
            .load::<(StoredGroupMessage, Option<i64>)>(conn)?
        {
            stored.insert(message.id.clone(), (message, delivery_sequence));
        }
    }
    Ok(stored)
}

#[cfg(test)]
use diesel::query_builder::{AstPass, Query, QueryFragment, QueryId};
#[cfg(test)]
use diesel::sql_types::{Integer, Text};
#[cfg(test)]
pub(super) struct Explain<T>(pub(super) T);
#[cfg(test)]
impl<T> QueryId for Explain<T> {
    type QueryId = ();
    const HAS_STATIC_QUERY_ID: bool = false;
}
#[cfg(test)]
impl<T> Query for Explain<T> {
    type SqlType = (Integer, Integer, Integer, Text);
}
#[cfg(test)]
impl<T: QueryFragment<Sqlite>> QueryFragment<Sqlite> for Explain<T> {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, Sqlite>) -> diesel::QueryResult<()> {
        pass.push_sql("EXPLAIN QUERY PLAN ");
        self.0.walk_ast(pass)
    }
}
#[cfg(test)]
impl<T> RunQueryDsl<diesel::SqliteConnection> for Explain<T> {}
