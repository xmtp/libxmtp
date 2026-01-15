use anyhow::Result;
use xmtp_db::{ConnectionExt, DbConnection, group_message::QueryGroupMessage};

use crate::confirm_destructive;

pub fn clear_all_messages<C: ConnectionExt>(
    conn: &C,
    limit_days: Option<u32>,
    group_ids: Option<&[Vec<u8>]>,
) -> Result<()> {
    confirm_destructive()?;
    clear_all_messages_confirmed(conn, limit_days, group_ids)
}

pub fn clear_all_messages_confirmed<C: ConnectionExt>(
    conn: &C,
    limit_days: Option<u32>,
    group_ids: Option<&[Vec<u8>]>,
) -> Result<()> {
    let db = DbConnection::new(conn);
    db.clear_messages(group_ids, limit_days)?;
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

        clear_all_messages_confirmed(
            &alix.db(),
            None,
            Some(std::slice::from_ref(&alix_bo_dm.group_id)),
        )?;
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

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_clear_messages_retention_days() {
        tester!(alix, disable_workers);
        tester!(bo);

        let dm = alix
            .find_or_create_dm_by_inbox_id(bo.inbox_id(), None)
            .await?;
        dm.send_message(b"Message 1", Default::default()).await?;
        dm.send_message(b"Message 2", Default::default()).await?;

        let initial_count = dm.find_messages(&MsgQueryArgs::default())?.len();
        // 1 commit message from DM creation + 2 application messages
        assert_eq!(initial_count, 3);

        // retention_days = 1 means "delete messages older than 1 day"
        // Since these messages were just created, they should NOT be deleted
        clear_all_messages_confirmed(&alix.db(), Some(1), None)?;
        let after_retention_1 = dm.find_messages(&MsgQueryArgs::default())?.len();
        assert_eq!(
            after_retention_1, initial_count,
            "Recent messages should not be deleted with retention_days=1"
        );

        // retention_days = 0 means "delete messages older than now"
        // All messages were created before now, so they should all be deleted
        clear_all_messages_confirmed(&alix.db(), Some(0), None)?;
        let after_retention_0 = dm.find_messages(&MsgQueryArgs::default())?.len();
        assert_eq!(
            after_retention_0, 0,
            "All messages should be deleted with retention_days=0"
        );
    }
}
