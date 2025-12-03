use anyhow::Result;
use xmtp_common::{NS_IN_DAY, time::now_ns};
use xmtp_db::{
    ConnectionExt,
    diesel::{self, ExpressionMethods, QueryDsl, RunQueryDsl},
    schema::group_messages::dsl as messages_dsl,
};

use crate::confirm_destructive;

pub fn clear_all_messages(conn: &impl ConnectionExt, limit_days: Option<i64>) -> Result<()> {
    confirm_destructive()?;
    clear_all_messages_confirmed(conn, limit_days)
}

fn clear_all_messages_confirmed(conn: &impl ConnectionExt, limit_days: Option<i64>) -> Result<()> {
    let mut query = diesel::delete(messages_dsl::group_messages).into_boxed();

    if let Some(days) = limit_days {
        let limit = now_ns() - NS_IN_DAY * days;
        query = query.filter(messages_dsl::sent_at_ns.lt(limit));
    }

    conn.raw_query_write(|c| query.execute(c))?;
    Ok(())
}

pub fn clear_all_messages_for_groups(
    conn: &impl ConnectionExt,
    group_ids: &[Vec<u8>],
    limit_days: Option<i64>,
) -> Result<()> {
    confirm_destructive()?;

    let mut query = diesel::delete(
        messages_dsl::group_messages.filter(messages_dsl::group_id.eq_any(group_ids)),
    )
    .into_boxed();

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

    use crate::tasks::clear_messages::clear_all_messages_confirmed;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_clear_msgs() {
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

        let alix_msgs = alix_bo_dm.find_messages(&MsgQueryArgs::default())?;
        // Commit and application msg
        assert_eq!(alix_msgs.len(), 2);

        clear_all_messages_confirmed(&alix.db(), None)?;
    }
}
