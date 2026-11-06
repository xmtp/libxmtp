//! The leaves of `groups/tests/test_welcomes.rs` that need the client.

use crate::tester;
use xmtp_db::DbQuery;
use xmtp_db::prelude::QueryRefreshState;
use xmtp_db::refresh_state::EntityKind;
use xmtp_proto::types::GroupId;

#[track_caller]
fn assert_cursors(db: &impl DbQuery, db2: &impl DbQuery, group_id: &GroupId) {
    let msg = db
        .get_group_messages(group_id, &Default::default())
        .unwrap();
    let msg = msg.last().unwrap();
    let cursor = db
        .get_last_cursor_for_ids(&[group_id.as_slice()], &[EntityKind::ApplicationMessage])
        .unwrap()
        .values()
        .next()
        .copied()
        .unwrap();

    assert_eq!(
        msg.cursor(),
        cursor,
        "the processed group cursor must match the last message"
    );

    let other_msg = db2
        .get_group_messages(group_id, &Default::default())
        .unwrap();
    let other_msg = other_msg.last().unwrap();
    assert_eq!(
        msg.cursor(),
        other_msg.cursor(),
        "GroupMessage must equal group message of db2"
    );
    let other_cursor = db2
        .get_last_cursor_for_ids(&[group_id.as_slice()], &[EntityKind::ApplicationMessage])
        .unwrap()
        .values()
        .next()
        .copied()
        .unwrap();
    assert_eq!(
        cursor, other_cursor,
        "the processed group cursors must be equal"
    );
}

// All members must derive the same state from the joining commit.
#[xmtp_common::test(unwrap_try = true)]
async fn test_inviting_members_results_in_consistent_state() {
    use EntityKind::ApplicationMessage;
    tester!(alix);
    tester!(bo);
    tester!(caro);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let group_id = &alix_group.group_id;
    assert_cursors(&alix.db(), &alix.db(), group_id);

    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    assert_cursors(&alix.db(), &bo.db(), group_id);

    alix_group.add_members(&[caro.inbox_id()]).await?;

    let caro_group = caro.wait_for_welcomes().await?.pop()?;
    alix_group.sync().await?;
    assert_cursors(&caro.db(), &caro.db(), group_id);
    assert_cursors(&caro.db(), &alix.db(), group_id);

    // ensure all groups have the latest
    bo_group.sync().await?;
    alix_group.sync().await?;
    caro_group.sync().await?;
    assert_cursors(&caro.db(), &caro.db(), group_id);
    assert_cursors(&caro.db(), &bo.db(), group_id);
    assert_cursors(&caro.db(), &alix.db(), group_id);

    // alix has the membership commit
    let alix_commit = alix
        .db()
        .get_last_cursor_for_ids(&[group_id], &[ApplicationMessage])?;
    let bo_commit = bo
        .db()
        .get_last_cursor_for_ids(&[group_id], &[ApplicationMessage])?;
    let caro_commit = caro
        .db()
        .get_last_cursor_for_ids(&[group_id], &[ApplicationMessage])?;
    assert_eq!(bo_commit, caro_commit);
    assert_eq!(alix_commit, bo_commit);
}
