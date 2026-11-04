//! Client group tests: conversation list.

use super::*;

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
async fn test_list_conversations_pagination() {
    use prost::Message;
    use xmtp_mls_group_state::group::GroupMetadataOptions;

    let alix = Tester::builder().build().await;
    let bo = Tester::builder().build().await;

    // Create 15 groups with small delays to ensure different created_at_ns values
    let mut all_group_ids = Vec::new();
    for i in 0..15 {
        let group = alix
            .create_group_with_members(
                &[bo.inbox_id().to_string()],
                None,
                Some(GroupMetadataOptions {
                    name: Some(format!("Group {}", i + 1)),
                    ..Default::default()
                }),
            )
            .await
            .unwrap();
        all_group_ids.push(group.group_id);
        group
            .send_message(
                TextCodec::encode("hello".to_string())
                    .unwrap()
                    .encode_to_vec()
                    .as_slice(),
                SendMessageOpts::default(),
            )
            .await
            .unwrap();
        // Small delay to ensure different timestamps
        xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let mut before = None;
    let mut all_conversation_ids = Vec::new();
    loop {
        let results = alix
            .list_conversations(GroupQueryArgs {
                limit: Some(5),
                last_activity_before_ns: before.map(|(time, _)| time),
                before_id: before.map(|(_, id)| id),
                ..Default::default()
            })
            .unwrap();

        if results.is_empty() {
            break;
        }
        assert_eq!(results.len(), 5);

        all_conversation_ids.extend(results.iter().map(|item| item.group.group_id));

        let last = results.last().unwrap();
        before = Some((last.listing_time_ns, last.group.group_id));
    }

    assert_eq!(
        all_conversation_ids.len(),
        15,
        "Should have 15 total conversations"
    );
    all_conversation_ids.dedup();

    // Check that we got all 15 unique groups
    assert_eq!(
        all_conversation_ids.len(),
        15,
        "Should have 15 total conversations after deduping"
    );
}

/// Store a conversation row with no MLS state; a listing reads only rows.
fn store_listed_group(
    db: &impl xmtp_db::ConnectionExt,
    id: xmtp_proto::types::GroupId,
    created_at_ns: i64,
) -> Result<(), xmtp_db::StorageError> {
    use xmtp_db::{Store, group::GroupMembershipState, group::StoredGroup};
    StoredGroup::builder()
        .id(id)
        .created_at_ns(created_at_ns)
        .membership_state(GroupMembershipState::Allowed)
        .added_by_inbox_id("peer")
        .build()
        .expect("group row")
        .store(db)
}

/// A peer can fill the default listing with Unknown-consent groups. A page
/// holds at most the app's limit, or 100, and carries each group's listing
/// time instead of its peer-sized latest message.
// verifies: CONS-046
#[xmtp_common::test(unwrap_try = true)]
async fn conversation_list_is_bounded_and_carries_listing_time() {
    use crate::client::ClientError;
    use xmtp_common::Generate;
    use xmtp_db::group_message::GroupMessageKind;
    use xmtp_proto::types::GroupId;

    const GROUP_COUNT: usize = 8;
    const TEXT_BYTES: usize = 128 * 1024;
    const SEEDED_ROWS: usize = 120;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);

    let message = TextCodec::encode("x".repeat(TEXT_BYTES))?.encode_to_vec();
    for _ in 0..GROUP_COUNT {
        let group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        group
            .send_message(&message, SendMessageOpts::default())
            .await?;
    }

    bo.sync_all_welcomes_and_groups(None).await?;
    let db = bo.context.db();
    for consent_states in [None, Some(vec![ConsentState::Unknown])] {
        let listed = bo.list_conversations(GroupQueryArgs {
            consent_states,
            ..Default::default()
        })?;
        assert_eq!(listed.len(), GROUP_COUNT);
        for item in &listed {
            let sent_at_ns = db
                .get_group_messages(
                    &item.group.group_id,
                    &MsgQueryArgs {
                        kind: Some(GroupMessageKind::Application),
                        ..Default::default()
                    },
                )?
                .iter()
                .map(|message| message.sent_at_ns)
                .max();
            assert_eq!(Some(item.listing_time_ns), sent_at_ns);
        }
    }

    for limit in [0, 1001] {
        let result = bo.list_conversations(GroupQueryArgs {
            limit: Some(limit),
            ..Default::default()
        });
        assert!(
            matches!(result, Err(ClientError::InvalidListOptions(_))),
            "limit {limit} was accepted"
        );
    }
    // Sync groups would bypass the page's limit and cursor.
    for args in [
        GroupQueryArgs {
            include_sync_groups: true,
            ..Default::default()
        },
        GroupQueryArgs {
            conversation_type: Some(xmtp_proto::types::ConversationType::Sync),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            bo.list_conversations(args),
            Err(ClientError::InvalidListOptions(_))
        ));
    }

    for _ in GROUP_COUNT..SEEDED_ROWS {
        store_listed_group(&db, GroupId::generate(), now_ns())?;
    }
    assert_eq!(bo.list_conversations(GroupQueryArgs::default())?.len(), 100);
    assert_eq!(
        bo.list_conversations(GroupQueryArgs {
            limit: Some(1000),
            ..Default::default()
        })?
        .len(),
        SEEDED_ROWS
    );
}

/// Pages resume after the last row of the previous page, even when many
/// groups share its listing time, so every group is returned once in each
/// order. Each page is the next slice of the full descending
/// (listing time, group id) order.
// verifies: CONS-047, CONS-049
#[xmtp_common::test(unwrap_try = true)]
async fn conversation_list_pages_resume_after_ties() {
    use crate::client::ClientError;
    use crate::test::mock::generate_stored_msg;
    use xmtp_db::{Store, group::GroupQueryOrderBy};
    use xmtp_proto::types::{Cursor, GroupId};

    const GROUPS: u8 = 120;
    const PAGE: i64 = 50;
    // Group `index` has id [index; 16]. Times fall as the index grows, and a
    // range of groups shares the time of its first group. Creation-time ties
    // fill rows 31 to 70 and activity-time ties rows 41 to 80, so the first
    // page ends inside a tie block in both orders.
    const CREATED_TIES: std::ops::Range<u8> = 30..70;
    const ACTIVITY_TIES: std::ops::Range<u8> = 40..80;
    // Includes group 70, the last row of the first activity page.
    const EXPIRED_NEWER: std::ops::Range<u8> = 65..75;
    let time = |base: i64, ties: &std::ops::Range<u8>, index: u8| {
        let rank = if ties.contains(&index) {
            ties.start
        } else {
            index
        };
        base - i64::from(rank) * 1_000
    };

    tester!(alix, disable_workers);
    let db = alix.context.db();
    let now = now_ns();
    for index in 0..GROUPS {
        let id = GroupId::from([index; 16]);
        store_listed_group(&db, id, time(2_000_000, &CREATED_TIES, index))?;
        let mut live = generate_stored_msg(Cursor(u64::from(index) + 1), id);
        live.sent_at_ns = time(9_000_000, &ACTIVITY_TIES, index);
        live.store(&db)?;
        if EXPIRED_NEWER.contains(&index) {
            let mut expired = generate_stored_msg(Cursor(u64::from(index) + 1_000), id);
            expired.sent_at_ns = 10_000_000;
            expired.expire_at_ns = Some(now - 1);
            expired.store(&db)?;
        }
    }

    for order_by in [
        GroupQueryOrderBy::CreatedAt,
        GroupQueryOrderBy::LastActivity,
    ] {
        let list = |limit, before: Option<(i64, GroupId)>| {
            let time = before.map(|(time, _)| time);
            let (created_before_ns, last_activity_before_ns) = match order_by {
                GroupQueryOrderBy::CreatedAt => (time, None),
                GroupQueryOrderBy::LastActivity => (None, time),
            };
            alix.list_conversations(GroupQueryArgs {
                limit: Some(limit),
                created_before_ns,
                last_activity_before_ns,
                before_id: before.map(|(_, id)| id),
                order_by: Some(order_by.clone()),
                ..Default::default()
            })
        };
        let full = list(1000, None)?;
        assert_eq!(full.len(), usize::from(GROUPS));
        let keys: Vec<_> = full
            .iter()
            .map(|item| (item.listing_time_ns, item.group.group_id))
            .collect();
        assert!(
            keys.windows(2).all(|pair| pair[0] > pair[1]),
            "{order_by:?} rows are not in descending (listing time, id) order"
        );

        let mut paged = Vec::new();
        let mut before = None;
        loop {
            let page = list(PAGE, before)?;
            let Some(last) = page.last() else { break };
            // The app's cursor is the creation time or the last-activity time.
            let time = match order_by {
                GroupQueryOrderBy::CreatedAt => last.group.created_at_ns,
                GroupQueryOrderBy::LastActivity => last.group.last_activity_ns(None)?,
            };
            before = Some((time, last.group.group_id));
            paged.extend(page.iter().map(|item| item.group.group_id));
            assert!(
                paged.len() <= usize::from(GROUPS),
                "{order_by:?} pages repeat groups"
            );
        }
        let full_ids: Vec<_> = full.iter().map(|item| item.group.group_id).collect();
        assert_eq!(paged, full_ids, "{order_by:?} pages skip or repeat a group");

        let result = alix.list_conversations(GroupQueryArgs {
            before_id: Some(full_ids[0]),
            order_by: Some(order_by.clone()),
            ..Default::default()
        });
        assert!(
            matches!(result, Err(ClientError::InvalidListOptions(_))),
            "{order_by:?} accepted before_id without a before time"
        );
    }
}
