use anyhow::Result;
use xmtp_common::{NS_IN_DAY, time::now_ns};
use xmtp_db::{
    ConnectionExt,
    diesel::{self, ExpressionMethods, RunQueryDsl},
    schema::group_messages::dsl as messages_dsl,
};

use crate::confirm_destructive;

pub fn clear_all_messages(
    conn: &impl ConnectionExt,
    limit_days: Option<i64>,
    group_ids: Option<&[Vec<u8>]>,
) -> Result<()> {
    confirm_destructive()?;
    clear_all_messages_confirmed(conn, limit_days, group_ids)
}

pub fn clear_all_messages_confirmed(
    conn: &impl ConnectionExt,
    limit_days: Option<i64>,
    group_ids: Option<&[Vec<u8>]>,
) -> Result<()> {
    let mut query = diesel::delete(messages_dsl::group_messages).into_boxed();

    if let Some(group_ids) = group_ids {
        query = query.filter(messages_dsl::group_id.eq_any(group_ids));
    }

    if let Some(days) = limit_days {
        let limit = now_ns() - NS_IN_DAY * days;
        query = query.filter(messages_dsl::sent_at_ns.lt(limit));
    }

    conn.raw_query_write(|c| query.execute(c))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use xmtp_db::group_message::MsgQueryArgs;
    use xmtp_mls::tester;

    use crate::tasks::clear_all_messages_confirmed;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_clear_msgs_and_groups_still_work() {
        tester!(alix, disable_workers);
        tester!(bo);

        let bo_alix_dm = bo
            .find_or_create_dm_by_inbox_id(alix.inbox_id(), None)
            .await?;
        bo_alix_dm
            .send_message(b"Hello there", Default::default())
            .await?;

        alix.sync_all_welcomes_and_groups(None).await?;
        let alix_bo_dm = alix.group(&bo_alix_dm.group_id)?;
        let alix_group = alix
            .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
            .await?;
        alix_group
            .send_message(b"This message needs to remain", Default::default())
            .await?;

        let alix_msgs = alix_bo_dm.find_messages(&MsgQueryArgs::default())?;
        // Commit and application msg
        assert_eq!(alix_msgs.len(), 2);

        clear_all_messages_confirmed(&alix.db(), None, Some(&[alix_bo_dm.group_id.clone()]))?;
        let alix_msgs = alix_bo_dm.find_messages(&MsgQueryArgs::default())?;
        // Commit and application msg
        assert_eq!(alix_msgs.len(), 0);

        let (dm, _) = alix.test_talk_in_dm_with(&bo).await?;
        assert_eq!(dm.group_id, alix_bo_dm.group_id);

        // These group messages should remain
        let group_msgs = alix_group.find_messages(&MsgQueryArgs::default())?;
        assert_eq!(group_msgs.len(), 2);

        // Now clear them all
        clear_all_messages_confirmed(&alix.db(), None, None)?;
        let group_msgs = alix_group.find_messages(&MsgQueryArgs::default())?;
        assert_eq!(group_msgs.len(), 0);
    }
}
