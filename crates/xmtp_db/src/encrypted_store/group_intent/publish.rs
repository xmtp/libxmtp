use super::*;

pub(super) fn last_publish_intent_id<C: ConnectionExt>(
    db: &DbConnection<C>,
    group_id: &[u8],
) -> Result<Option<ID>, crate::ConnectionError> {
    db.raw_query(|conn| {
        dsl::group_intents
            .filter(dsl::group_id.eq(group_id))
            .filter(dsl::state.eq_any([IntentState::ToPublish, IntentState::Published]))
            .filter(dsl::kind.eq_any(IntentKind::all().collect::<Vec<_>>()))
            .select(diesel::dsl::max(dsl::id))
            .first(conn)
    })
}

pub(super) fn published_group_change_id<C: ConnectionExt>(
    db: &DbConnection<C>,
    group_id: &[u8],
) -> Result<Option<ID>, crate::ConnectionError> {
    db.raw_query(|conn| {
        dsl::group_intents
            .filter(dsl::group_id.eq(group_id))
            .filter(dsl::state.eq(IntentState::Published))
            .filter(dsl::kind.ne(IntentKind::SendMessage))
            .filter(dsl::kind.eq_any(IntentKind::all().collect::<Vec<_>>()))
            .order(dsl::id.asc())
            .select(dsl::id)
            .first(conn)
            .optional()
    })
}

pub(super) fn next_publish_intent<C: ConnectionExt>(
    db: &DbConnection<C>,
    group_id: &[u8],
    after: Option<ID>,
    upper: ID,
) -> Result<Option<StoredGroupIntent>, crate::ConnectionError> {
    // The group/state index keeps each state-specific seek in ID order.
    // Combining the states would require sorting the remaining rows.
    let id = if let Some(blocker) = published_group_change_id(db, group_id)? {
        Some(blocker)
    } else {
        let mut first = None;
        for state in [IntentState::ToPublish, IntentState::Published] {
            let next = db.raw_query(|conn| {
                dsl::group_intents
                    .filter(dsl::group_id.eq(group_id))
                    .filter(dsl::state.eq(state))
                    .filter(dsl::kind.eq_any(IntentKind::all().collect::<Vec<_>>()))
                    .filter(dsl::id.gt(after.unwrap_or(ID::MIN)))
                    .filter(dsl::id.le(upper))
                    .order(dsl::id.asc())
                    .select(dsl::id)
                    .first::<ID>(conn)
                    .optional()
            })?;
            if let Some(next) = next {
                first = Some(first.map_or(next, |id: ID| id.min(next)));
            }
        }
        first
    };
    let Some(id) = id else { return Ok(None) };
    db.raw_query(|conn| {
        dsl::group_intents
            .find(id)
            .select(StoredGroupIntent::as_select())
            .first(conn)
            .optional()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Store,
        group::{GroupMembershipState, StoredGroup},
        test_utils::with_connection,
    };
    use xmtp_common::Generate;

    fn insert_group<C: ConnectionExt>(db: &DbConnection<C>) -> GroupId {
        let id = GroupId::generate();
        StoredGroup::builder()
            .id(id)
            .created_at_ns(100)
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id("placeholder_address")
            .build()
            .unwrap()
            .store(db)
            .unwrap();
        id
    }

    fn intent<C: ConnectionExt>(
        db: &DbConnection<C>,
        group: GroupId,
        kind: IntentKind,
        state: IntentState,
    ) -> StoredGroupIntent {
        db.insert_group_intent(NewGroupIntent::new_test(kind, group, vec![1, 2, 3], state))
            .unwrap()
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn publish_selection_preserves_fence_and_state_order() {
        with_connection(|db| {
            let group = insert_group(db);
            assert_eq!(db.last_publish_intent_id(group.as_ref()).unwrap(), None);
            let first = intent(db, group, IntentKind::SendMessage, IntentState::Published);
            intent(db, group, IntentKind::SendMessage, IntentState::Error);
            let second = intent(db, group, IntentKind::SendMessage, IntentState::ToPublish);
            intent(db, group, IntentKind::KeyUpdate, IntentState::Committed);
            let third = intent(
                db,
                group,
                IntentKind::MetadataUpdate,
                IntentState::ToPublish,
            );
            let upper = db.last_publish_intent_id(group.as_ref()).unwrap().unwrap();
            assert_eq!(upper, third.id);
            intent(db, group, IntentKind::SendMessage, IntentState::ToPublish);
            let other_group = insert_group(db);
            intent(
                db,
                other_group,
                IntentKind::KeyUpdate,
                IntentState::Published,
            );
            let mut after = None;
            for expected in [first, second, third] {
                let selected = db
                    .next_publish_intent(group.as_ref(), after, upper)
                    .unwrap()
                    .unwrap();
                assert_eq!(selected.id, expected.id);
                assert_eq!(selected.data, expected.data);
                assert_eq!(selected.state, expected.state);
                after = Some(selected.id);
            }
            assert!(
                db.next_publish_intent(group.as_ref(), after, upper)
                    .unwrap()
                    .is_none()
            );
            assert!(!db.has_published_group_change(group.as_ref()).unwrap());
        });
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn published_group_change_preempts_cursor_and_fence() {
        with_connection(|db| {
            let group = insert_group(db);
            let message = intent(db, group, IntentKind::SendMessage, IntentState::ToPublish);
            let upper = db.last_publish_intent_id(group.as_ref()).unwrap().unwrap();
            let blocker = intent(
                db,
                group,
                IntentKind::MetadataUpdate,
                IntentState::Published,
            );
            let later = intent(db, group, IntentKind::KeyUpdate, IntentState::Published);
            let selected = db
                .next_publish_intent(group.as_ref(), Some(upper), upper)
                .unwrap()
                .unwrap();
            assert_eq!(selected.id, blocker.id);
            assert!(db.has_published_group_change(group.as_ref()).unwrap());
            db.set_group_intent_error(blocker.id).unwrap();
            assert_eq!(
                db.next_publish_intent(group.as_ref(), None, upper)
                    .unwrap()
                    .unwrap()
                    .id,
                later.id
            );
            db.set_group_intent_error(later.id).unwrap();
            assert_eq!(
                db.next_publish_intent(group.as_ref(), None, upper)
                    .unwrap()
                    .unwrap()
                    .id,
                message.id
            );
            assert!(!db.has_published_group_change(group.as_ref()).unwrap());
        });
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn publish_queries_exclude_unknown_kinds() {
        with_connection(|db| {
            let group = insert_group(db);
            let known = intent(db, group, IntentKind::SendMessage, IntentState::ToPublish);
            let unknown = intent(db, group, IntentKind::KeyUpdate, IntentState::Published);
            db.raw_query(|conn| {
                diesel::update(dsl::group_intents.find(unknown.id))
                    .set(dsl::kind.eq(999_i32))
                    .execute(conn)
            })
            .unwrap();
            assert_eq!(
                db.last_publish_intent_id(group.as_ref()).unwrap(),
                Some(known.id)
            );
            assert_eq!(
                db.next_publish_intent(group.as_ref(), None, unknown.id)
                    .unwrap()
                    .unwrap()
                    .id,
                known.id
            );
            assert!(
                db.next_publish_intent(group.as_ref(), Some(known.id), unknown.id)
                    .unwrap()
                    .is_none()
            );
            assert!(!db.has_published_group_change(group.as_ref()).unwrap());
        });
    }
}
