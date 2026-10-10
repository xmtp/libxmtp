#[cfg(test)]
use super::page::BEFORE_BODIES;
use super::page::{load_bodies, select_keys};
use super::*;
use crate::delivery::{
    AppVisibleMessageRow, HistoryPageRows, HistoryPosition, database_id, resolve_group_scope,
    validate_cursor,
};
use crate::{StorageError, stream_storage::StreamStorageError};
use std::collections::VecDeque;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Key {
    sent_at_ns: i64,
    delivery_sequence: i64,
    id: Vec<u8>,
}

macro_rules! key_query {
    ($group:expr, $args:expr, $limit:expr, $now:expr) => {{
        let group = $group;
        let args = $args;
        let query = dsl::group_messages
            .filter(dsl::group_id.eq(group.as_ref()))
            .filter(dsl::delivery_sequence.is_not_null())
            .filter(dsl::delivery_status.eq(DeliveryStatus::Published))
            .into_boxed();
        let query = apply_message_filters!(query, args, $now);
        let query = match args.direction.clone().unwrap_or_default() {
            SortDirection::Ascending => {
                query.order((dsl::sent_at_ns.asc(), dsl::delivery_sequence.asc()))
            }
            SortDirection::Descending => {
                query.order((dsl::sent_at_ns.desc(), dsl::delivery_sequence.desc()))
            }
        };
        query.limit($limit).select((
            dsl::id,
            dsl::sent_at_ns,
            dsl::delivery_sequence.assume_not_null(),
        ))
    }};
}

fn keys(
    conn: &mut diesel::SqliteConnection,
    group: &GroupId,
    args: &MsgQueryArgs,
    limit: i64,
    read_now: i64,
) -> Result<VecDeque<Key>, diesel::result::Error> {
    Ok(key_query!(group, args, limit, read_now)
        .load::<(Vec<u8>, i64, i64)>(conn)?
        .into_iter()
        .map(|(id, sent_at_ns, delivery_sequence)| Key {
            id,
            sent_at_ns,
            delivery_sequence,
        })
        .collect())
}

/// The caller holds one read transaction for scope, positions, keys and bodies.
pub(crate) fn read_history_page(
    conn: &mut diesel::SqliteConnection,
    group: &GroupId,
    args: &MsgQueryArgs,
) -> Result<HistoryPageRows, StorageError> {
    let limit = args
        .limit
        .filter(|value| *value > 0)
        .ok_or(StreamStorageError::InvalidDeliveryPosition)?;
    let selector_limit = limit
        .checked_add(1)
        .ok_or(StreamStorageError::InvalidDeliveryPosition)?;
    for position in [args.history_before, args.history_after]
        .into_iter()
        .flatten()
    {
        if position.cursor.delivery_sequence == 0 {
            return Err(StreamStorageError::InvalidDeliveryPosition.into());
        }
        validate_cursor(conn, position.cursor)?;
    }
    let identity = database_id(conn)?;
    let scope = resolve_group_scope(conn, &[*group])?;
    let read_now = now_ns();
    let ascending = args.direction.clone().unwrap_or_default() == SortDirection::Ascending;
    let mut sources = Vec::with_capacity(scope.group_ids.len());
    for physical_group in &scope.group_ids {
        sources.push(keys(conn, physical_group, args, selector_limit, read_now)?);
    }
    #[cfg(test)]
    let candidate_keys = sources.iter().map(VecDeque::len).sum();
    let (selected, has_more) = select_keys(sources, limit, ascending);
    let position = |key: &Key| HistoryPosition {
        sent_at_ns: key.sent_at_ns,
        cursor: crate::delivery::DeliveryCursor {
            database_id: identity,
            delivery_sequence: key.delivery_sequence as u64,
        },
    };
    let first_position = selected.first().map(&position);
    let last_position = selected.last().map(&position);
    let ids = selected
        .iter()
        .map(|key| key.id.as_slice())
        .collect::<Vec<_>>();
    let mut stored = load_bodies(conn, &ids)?;
    #[cfg(test)]
    let base_bodies_loaded = stored.len();
    let rows = selected
        .into_iter()
        .map(|key| {
            let (stored, _) = stored
                .remove(&key.id)
                .ok_or(diesel::result::Error::NotFound)?;
            Ok(AppVisibleMessageRow {
                stored,
                cursor: Some(position(&key).cursor),
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(HistoryPageRows {
        rows,
        first_position,
        last_position,
        has_more,
        #[cfg(test)]
        candidate_keys,
        #[cfg(test)]
        base_bodies_loaded,
        #[cfg(test)]
        physical_groups: scope.group_ids.len(),
    })
}

#[cfg(test)]
mod tests;
