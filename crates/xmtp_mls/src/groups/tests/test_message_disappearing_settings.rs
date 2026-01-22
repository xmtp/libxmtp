use xmtp_db::group_message::GroupMessageKind;

use crate::{messages::decoded_message::MessageBody, tester};

#[xmtp_common::test(unwrap_try = true)]
async fn test_disappearing_message_update_message_in_group() {
    tester!(alix);
    tester!(bo);

    let alix_bo_dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let _bo_alix_dm = bo.find_or_create_dm(alix.inbox_id(), None).await?;

    alix_bo_dm
        .update_conversation_message_disappear_from_ns(10)
        .await?;

    alix.sync_all_welcomes_and_groups(None).await?;

    let msgs = alix_bo_dm.find_messages_v2(&Default::default())?;

    // Two group updated messages:
    // 1. Added Bo
    // 2. Updated disappearing message setting
    assert_eq!(msgs[0].metadata.kind, GroupMessageKind::MembershipChange);
    assert!(matches!(msgs[1].content, MessageBody::GroupUpdated(_)));
    assert_eq!(msgs[2].metadata.kind, GroupMessageKind::MembershipChange);
    assert_eq!(msgs.len(), 3);

    let alix_bo_alix_dm = alix.group(&_bo_alix_dm.group_id)?;
    let msgs = alix_bo_alix_dm.find_messages_v2(&Default::default())?;
    assert_eq!(msgs.len(), 3);
}
