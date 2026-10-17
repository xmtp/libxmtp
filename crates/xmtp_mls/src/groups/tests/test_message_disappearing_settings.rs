use std::time::Duration;

use xmtp_common::{NS_IN_HOUR, time::now_ns};
use xmtp_db::group_message::{GroupMessageKind, MsgQueryArgs};
use xmtp_db::prelude::*;
use xmtp_mls_common::{
    group::GroupMetadataOptions, group_mutable_metadata::MessageDisappearingSettings,
};

use crate::{groups::GroupError, messages::decoded_message::MessageBody, tester};

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

/// An application message expires at its backend sent time plus the duration in
/// force when it is first stored, on the sender and on a recipient that receives it
/// late. Processing-time arithmetic would give the late recipient a longer lifespan.
/// A message sent before `from_ns` gets no expiry, a sum past `i64::MAX` clamps, a
/// later settings change never moves a stored deadline, and membership-change
/// messages never expire.
// verifies: META-050
#[xmtp_common::test(unwrap_try = true)]
async fn message_expiry_from_sent_time() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(
        None,
        Some(GroupMetadataOptions {
            message_disappearing_settings: Some(MessageDisappearingSettings::new(1, NS_IN_HOUR)),
            ..Default::default()
        }),
    )?;
    group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let bo_group = bo.group(&group.group_id)?;

    let (alix_db, bo_db) = (alix.context.db(), bo.context.db());
    let deadlines = |id: &[u8]| {
        let [sent, received] =
            [&alix_db, &bo_db].map(|db| db.get_group_message(id).unwrap().unwrap());
        assert_eq!(sent.sent_at_ns, received.sent_at_ns);
        (sent.sent_at_ns, [sent.expire_at_ns, received.expire_at_ns])
    };
    let send = async |content: &[u8]| {
        let id = group.send_message(content, Default::default()).await?;
        xmtp_common::time::sleep(Duration::from_millis(100)).await;
        bo_group.sync().await?;
        Ok::<_, GroupError>(id)
    };

    let delayed = send(b"delayed").await?;
    let (sent_at, expiries) = deadlines(&delayed);
    assert_eq!(expiries, [Some(sent_at + NS_IN_HOUR); 2]);

    group
        .update_conversation_message_disappearing_settings(MessageDisappearingSettings::new(
            1,
            2 * NS_IN_HOUR,
        ))
        .await?;
    let longer = send(b"longer").await?;
    let (longer_sent_at, expiries) = deadlines(&longer);
    assert_eq!(expiries, [Some(longer_sent_at + 2 * NS_IN_HOUR); 2]);
    assert_eq!(
        deadlines(&delayed),
        (sent_at, [Some(sent_at + NS_IN_HOUR); 2])
    );

    group
        .update_conversation_message_disappearing_settings(MessageDisappearingSettings::new(
            now_ns() + NS_IN_HOUR,
            NS_IN_HOUR,
        ))
        .await?;
    let early = send(b"before from_ns").await?;
    assert_eq!(deadlines(&early).1, [None; 2]);

    group
        .update_conversation_message_disappearing_settings(MessageDisappearingSettings::new(
            1,
            i64::MAX,
        ))
        .await?;
    let forever = send(b"forever").await?;
    assert_eq!(deadlines(&forever).1, [Some(i64::MAX); 2]);
    assert_eq!(
        deadlines(&delayed),
        (sent_at, [Some(sent_at + NS_IN_HOUR); 2])
    );

    for db in [&alix_db, &bo_db] {
        let updates = db.get_group_messages(
            &group.group_id,
            &MsgQueryArgs {
                kind: Some(GroupMessageKind::MembershipChange),
                ..Default::default()
            },
        )?;
        assert!(!updates.is_empty());
        assert!(updates.iter().all(|m| m.expire_at_ns.is_none()));
    }
}
