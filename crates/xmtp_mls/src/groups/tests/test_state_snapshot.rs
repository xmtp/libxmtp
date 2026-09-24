use crate::client::notifications::{
    NotificationChannel, NotificationConfig, NotificationOverride, encode,
};
use crate::context::XmtpSharedContext;
use crate::groups::{GroupError, SendMessageOpts, UpdateAdminListType};
use crate::tester;
use crate::utils::TestMlsGroup;
use xmtp_content_types::{
    ContentCodec, encoded_content_to_bytes,
    read_receipt::{ReadReceipt, ReadReceiptCodec},
    text::TextCodec,
};
use xmtp_db::consent_record::ConsentState;
use xmtp_db::encrypted_store::database::count_sql_queries;
use xmtp_db::group::{GroupQueryArgs, GroupQueryOrderBy};
use xmtp_db::group_message::{ContentType, GroupMessageKind};
use xmtp_db::prelude::*;
use xmtp_db::sql_key_store::count_kv_reads;

fn assert_notifications(group: &TestMlsGroup, expected: bool) -> Result<(), GroupError> {
    assert_eq!(group.state_snapshot()?.notifications_enabled, expected);
    assert_eq!(group.notifications_enabled()?, expected);
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_state_snapshot_read_count() {
    tester!(alix);
    tester!(bo);
    let group = alix.create_group(None, None)?;
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;

    for conversation in [&group, &dm] {
        let ((result, queries, begins), kv_reads) =
            count_kv_reads(|| count_sql_queries(|| conversation.state_snapshot()));
        let state = result?;
        assert!(kv_reads <= 2, "snapshot read {kv_reads} MLS keys");
        assert!(
            queries - kv_reads <= 3,
            "snapshot ran {queries} SQL statements, {kv_reads} for MLS keys"
        );
        assert_eq!(begins, 0, "snapshot began a write transaction");
        assert_eq!(
            state.group.is_some(),
            conversation.group_id == group.group_id
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_state_snapshot_matches_getters() {
    tester!(alix);
    tester!(bo);
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;
    group
        .update_admin_list(UpdateAdminListType::Add, bo.inbox_id().to_string())
        .await?;
    group.update_group_name("snapshot name".into()).await?;
    group
        .update_group_description("snapshot description".into())
        .await?;
    group
        .update_group_image_url_square("https://example.com/image".into())
        .await?;
    group.update_app_data("snapshot data".into(), None).await?;
    group
        .update_conversation_message_disappearing_settings(
            xmtp_mls_common::group_mutable_metadata::MessageDisappearingSettings::new(1, 2),
        )
        .await?;

    let db = alix.context.db();
    let mut notification = db.notification_record()?;
    notification.push_state = 1;
    notification.push_config = Some(encode(&NotificationConfig::new(
        NotificationChannel::Fcm {
            token: "snapshot-test".into(),
        },
    ))?);
    db.save_notification_record(&notification)?;

    group.update_consent_state(ConsentState::Unknown)?;
    group.set_notifications(NotificationOverride::Default)?;
    assert_notifications(&group, false)?;
    group.update_consent_state(ConsentState::Allowed)?;
    assert_notifications(&group, true)?;
    group.update_consent_state(ConsentState::Denied)?;
    assert_notifications(&group, false)?;
    group.set_notifications(NotificationOverride::Enabled)?;
    assert_notifications(&group, true)?;

    let state = group.state_snapshot()?;
    assert_eq!(state.is_active, group.is_active()?);
    assert_eq!(state.consent_state, group.consent_state()?);
    assert_eq!(state.paused_for_version, group.paused_for_version()?);
    assert_eq!(state.disappearing_settings, group.disappearing_settings()?);
    assert!(state.is_disappearing_enabled);
    assert_eq!(state.notifications_enabled, group.notifications_enabled()?);
    assert_eq!(
        state.commit_log_fork_status,
        alix.context
            .db()
            .find_group(&group.group_id)?
            .unwrap()
            .is_commit_log_forked
    );
    let metadata = state.group.unwrap();
    assert_eq!(metadata.name, group.group_name()?);
    assert_eq!(metadata.image_url, group.group_image_url_square()?);
    assert_eq!(metadata.description, group.group_description()?);
    assert_eq!(metadata.app_data, group.app_data()?);
    assert_eq!(metadata.membership_state, group.membership_state()?);
    assert_eq!(metadata.admins, group.admin_list()?);
    assert_eq!(metadata.super_admins, group.super_admin_list()?);
    assert_eq!(metadata.permissions, group.permissions()?);
    assert_eq!(
        metadata.policy_type,
        group.permissions()?.preconfigured_policy().ok()
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_last_activity_matches_list_order() {
    tester!(alix);
    tester!(bo);
    let empty = alix.create_group(None, None)?;
    let text_then_receipt = alix.create_group(None, None)?;
    let member_change = alix.create_group(None, None)?;
    let receipt_only = alix.create_group(None, None)?;

    let text = encoded_content_to_bytes(TextCodec::encode("visible".into())?);
    let receipt = encoded_content_to_bytes(ReadReceiptCodec::encode(ReadReceipt {})?);
    text_then_receipt
        .send_message(&text, SendMessageOpts::default())
        .await?;
    let visible_time = text_then_receipt.last_activity_ns(None)?;
    text_then_receipt
        .send_message(&receipt, SendMessageOpts::default())
        .await?;
    let receipt_time = text_then_receipt.last_activity_ns(Some(&[ContentType::ReadReceipt]))?;
    assert!(receipt_time > visible_time);

    member_change
        .send_message(&text, SendMessageOpts::default())
        .await?;
    let before_add = member_change.last_activity_ns(None)?;
    member_change.add_members(&[bo.inbox_id()]).await?;
    let member_row = alix
        .context
        .db()
        .find_group(&member_change.group_id)?
        .unwrap();
    let member_latest = member_row.last_message_ns.unwrap();
    assert!(member_latest > before_add);
    assert!(
        member_change
            .find_messages(&Default::default())?
            .iter()
            .any(|message| {
                message.sent_at_ns == member_latest
                    && message.kind == GroupMessageKind::MembershipChange
            })
    );

    receipt_only
        .send_message(&receipt, SendMessageOpts::default())
        .await?;
    let receipt_only_time = receipt_only.last_activity_ns(Some(&[ContentType::ReadReceipt]))?;
    assert!(receipt_only_time > receipt_only.created_at_ns);

    assert_eq!(empty.last_activity_ns(None)?, empty.created_at_ns);
    let ordered = alix.context.db().fetch_conversation_list(GroupQueryArgs {
        order_by: Some(GroupQueryOrderBy::LastActivity),
        ..Default::default()
    })?;
    let expected = [&empty, &text_then_receipt, &member_change, &receipt_only];
    for group in expected {
        let row = ordered.iter().find(|row| row.id == group.group_id).unwrap();
        assert_eq!(
            group.last_activity_ns(None)?,
            row.sent_at_ns.unwrap_or(row.created_at_ns)
        );
    }
    for group in [&text_then_receipt, &member_change, &receipt_only] {
        let row = ordered.iter().find(|row| row.id == group.group_id).unwrap();
        let list_key = row.sent_at_ns.unwrap_or(row.created_at_ns);
        let last_message_ns = alix
            .context
            .db()
            .find_group(&group.group_id)?
            .unwrap()
            .last_message_ns
            .unwrap();
        assert_ne!(list_key, last_message_ns);
    }
    assert_eq!(text_then_receipt.last_activity_ns(None)?, visible_time);
    assert_eq!(member_change.last_activity_ns(None)?, before_add);
    assert_eq!(
        receipt_only.last_activity_ns(None)?,
        receipt_only.created_at_ns
    );
    for pair in ordered.windows(2) {
        assert!(
            pair[0].sent_at_ns.unwrap_or(pair[0].created_at_ns)
                >= pair[1].sent_at_ns.unwrap_or(pair[1].created_at_ns)
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_last_activity_custom_content_types() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let text = encoded_content_to_bytes(TextCodec::encode("typed".into())?);
    let receipt = encoded_content_to_bytes(ReadReceiptCodec::encode(ReadReceipt {})?);
    group
        .send_message(&text, SendMessageOpts::default())
        .await?;
    let text_time = group.last_activity_ns(None)?;
    group
        .send_message(&receipt, SendMessageOpts::default())
        .await?;
    let receipt_time = group.last_activity_ns(Some(&[ContentType::ReadReceipt]))?;
    assert!(receipt_time > text_time);
    assert_eq!(group.last_activity_ns(None)?, text_time);
    assert_eq!(group.last_activity_ns(Some(&[]))?, group.created_at_ns);
    assert_eq!(
        group.last_activity_ns(Some(&[ContentType::Text]))?,
        text_time
    );
    assert_eq!(
        group.last_activity_ns(Some(&[ContentType::ReadReceipt]))?,
        receipt_time
    );
    assert_eq!(
        group.last_activity_ns(Some(&[ContentType::Text, ContentType::ReadReceipt]))?,
        receipt_time
    );
}
