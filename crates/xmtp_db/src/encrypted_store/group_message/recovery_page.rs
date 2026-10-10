use super::page::{load_bodies, select_keys};
use super::*;
use crate::delivery::{AppVisibleMessageRow, database_id, resolve_group_scope};
use crate::{StorageError, stream_storage::StreamStorageError};
use std::collections::VecDeque;

/// Raw recovery boundary. Message ID validation must not erase unreadable keys.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryPosition {
    pub sent_at_ns: i64,
    pub database_id: [u8; 16],
    pub message_id: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct RecoveryQueryArgs {
    pub messages: MsgQueryArgs,
    pub before: Option<RecoveryPosition>,
    pub after: Option<RecoveryPosition>,
}

#[derive(Debug)]
pub struct RecoveryPageRows {
    pub rows: Vec<AppVisibleMessageRow>,
    pub first_position: Option<RecoveryPosition>,
    pub last_position: Option<RecoveryPosition>,
    pub has_more: bool,
    #[cfg(test)]
    pub candidate_keys: usize,
    #[cfg(test)]
    pub base_bodies_loaded: usize,
    #[cfg(test)]
    pub physical_groups: usize,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Key {
    sent_at_ns: i64,
    id: Vec<u8>,
}

macro_rules! key_query {
    ($group:expr, $args:expr, $limit:expr, $now:expr) => {{
        let args = $args;
        // Keep the literal predicate equal to the partial index predicate.
        let query = dsl::group_messages
            .filter(dsl::group_id.eq($group.as_ref()))
            .filter(diesel_sql::<diesel::sql_types::Bool>(
                "delivery_status IN (1, 3)",
            ))
            .into_boxed();
        let mut query = apply_message_filters!(query, &args.messages, $now);
        for (position, operator) in [(&args.before, " < ("), (&args.after, " > (")] {
            if let Some(position) = position {
                query = query.filter(
                    diesel_sql::<diesel::sql_types::Bool>("(sent_at_ns, id)")
                        .sql(operator)
                        .bind::<diesel::sql_types::BigInt, _>(position.sent_at_ns)
                        .sql(",")
                        .bind::<diesel::sql_types::Binary, _>(&position.message_id)
                        .sql(")"),
                );
            }
        }
        let query = match args.messages.direction.clone().unwrap_or_default() {
            SortDirection::Ascending => query.order((dsl::sent_at_ns.asc(), dsl::id.asc())),
            SortDirection::Descending => query.order((dsl::sent_at_ns.desc(), dsl::id.desc())),
        };
        query.limit($limit).select((dsl::sent_at_ns, dsl::id))
    }};
}

/// Scope, UUID, keys and bodies use the caller's single read transaction.
pub(crate) fn read_recovery_page(
    conn: &mut diesel::SqliteConnection,
    group: &GroupId,
    args: &RecoveryQueryArgs,
) -> Result<RecoveryPageRows, StorageError> {
    let limit = args
        .messages
        .limit
        .filter(|value| *value > 0)
        .ok_or(StreamStorageError::InvalidDeliveryPosition)?;
    let selector_limit = limit
        .checked_add(1)
        .ok_or(StreamStorageError::InvalidDeliveryPosition)?;
    let identity = database_id(conn)?;
    for position in [&args.before, &args.after].into_iter().flatten() {
        if position.database_id != identity {
            return Err(StreamStorageError::ForeignCursor.into());
        }
    }
    let scope = resolve_group_scope(conn, &[*group])?;
    let read_now = now_ns();
    let mut sources = Vec::with_capacity(scope.group_ids.len());
    for physical_group in &scope.group_ids {
        let keys = key_query!(physical_group, args, selector_limit, read_now)
            .load::<(i64, Vec<u8>)>(conn)?
            .into_iter()
            .map(|(sent_at_ns, id)| Key { sent_at_ns, id })
            .collect::<VecDeque<_>>();
        sources.push(keys);
    }
    #[cfg(test)]
    let candidate_keys = sources.iter().map(VecDeque::len).sum();
    let ascending = args.messages.direction.clone().unwrap_or_default() == SortDirection::Ascending;
    let (selected, has_more) = select_keys(sources, limit, ascending);
    let position = |key: &Key| RecoveryPosition {
        sent_at_ns: key.sent_at_ns,
        database_id: identity,
        message_id: key.id.clone(),
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
            let (stored, delivery_sequence) = stored
                .remove(&key.id)
                .ok_or(diesel::result::Error::NotFound)?;
            Ok(AppVisibleMessageRow {
                stored,
                // implements: PROC-050
                cursor: delivery_sequence.map(|sequence| crate::delivery::DeliveryCursor {
                    database_id: identity,
                    delivery_sequence: sequence as u64,
                }),
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(RecoveryPageRows {
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
