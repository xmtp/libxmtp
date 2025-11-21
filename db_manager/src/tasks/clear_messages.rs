use anyhow::Result;
use xmtp_db::{
    ConnectionExt, EncryptedMessageStore, NativeDb,
    diesel::{self, ExpressionMethods, QueryDsl, RunQueryDsl},
    schema::group_messages::dsl as messages_dsl,
};

use crate::confirm_destructive;

pub fn clear_all_messages(store: &EncryptedMessageStore<NativeDb>) -> Result<()> {
    confirm_destructive()?;

    store
        .conn()
        .raw_query_write(|c| diesel::delete(messages_dsl::group_messages).execute(c))?;
    Ok(())
}

pub fn clear_all_messages_for_groups(
    store: &EncryptedMessageStore<NativeDb>,
    group_ids: &[Vec<u8>],
) -> Result<()> {
    confirm_destructive()?;

    for group_id in group_ids {
        store.conn().raw_query_write(|c| {
            diesel::delete(messages_dsl::group_messages.filter(messages_dsl::group_id.eq(group_id)))
                .execute(c)
        })?;
    }

    Ok(())
}
