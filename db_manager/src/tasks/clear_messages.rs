use anyhow::Result;
use xmtp_common::{NS_IN_DAY, time::now_ns};
use xmtp_db::{
    ConnectionExt, EncryptedMessageStore, NativeDb,
    diesel::{self, ExpressionMethods, QueryDsl, RunQueryDsl},
    schema::group_messages::dsl as messages_dsl,
};

use crate::confirm_destructive;

pub fn clear_all_messages(
    store: &EncryptedMessageStore<NativeDb>,
    limit_days: Option<i64>,
) -> Result<()> {
    confirm_destructive()?;

    let mut query = diesel::delete(messages_dsl::group_messages).into_boxed();

    if let Some(days) = limit_days {
        let limit = now_ns() - NS_IN_DAY * days;
        query = query.filter(messages_dsl::sent_at_ns.lt(limit));
    }

    store.conn().raw_query_write(|c| query.execute(c))?;
    Ok(())
}

pub fn clear_all_messages_for_groups(
    store: &EncryptedMessageStore<NativeDb>,
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

    store.conn().raw_query_write(|c| query.execute(c))?;

    Ok(())
}
