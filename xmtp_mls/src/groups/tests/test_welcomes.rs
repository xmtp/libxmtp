use crate::tester;
use xmtp_db::prelude::QueryRefreshState;
use xmtp_db::refresh_state::EntityKind;

#[xmtp_common::test(unwrap_try = true)]
async fn test_welcome_cursor() {
    // Welcomes now come with a cursor so that clients no longer pull down
    // every message in a group that they cannot decrypt.
    // This tests checks that cursor is being consumed from the welcome.
    tester!(alix);
    tester!(bo);

    let (group, _msg) = alix.test_talk_in_new_group_with(&bo).await?;

    tester!(alix2, from: alix);
    group.update_installations().await?;

    alix2.sync_welcomes().await?;
    let alix2_group = alix2.group(&group.group_id)?;
    let alix2_refresh_state = alix2
        .context
        .db()
        .get_refresh_state(&group.group_id, EntityKind::Group)??;

    assert!(alix2_refresh_state.cursor > 0);
}
