use xmtp_db::group_message::GroupMessageKind;

use crate::{messages::decoded_message::MessageBody, tester};

#[xmtp_common::test(unwrap_try = true)]
async fn test_disappearing_message_update_message_in_group() {
    tester!(alix);
    tester!(bo);

    let dm = alix
        .find_or_create_dm_by_inbox_id(bo.inbox_id(), None)
        .await?;
    dm.update_conversation_message_disappear_from_ns(10).await?;
    let msgs = dm.find_messages_v2(&Default::default())?;

    // Two group updated messages:
    // 1. Added Bo
    // 2. Updated disappearing message setting
    assert_eq!(msgs[0].metadata.kind, GroupMessageKind::MembershipChange);
    assert!(matches!(msgs[1].content, MessageBody::GroupUpdated(_)));
    assert_eq!(msgs.len(), 2);
}
