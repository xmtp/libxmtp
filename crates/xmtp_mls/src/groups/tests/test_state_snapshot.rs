use crate::client::notifications::NotificationOverride;
use crate::context::XmtpSharedContext;
use crate::groups::{SendMessageOpts, UpdateAdminListType};
use crate::tester;
use xmtp_db::consent_record::ConsentState;
use xmtp_db::encrypted_store::database::count_sql_queries;
use xmtp_db::group::{GroupQueryArgs, GroupQueryOrderBy};
use xmtp_db::group_message::ContentType;
use xmtp_db::prelude::*;
use xmtp_db::sql_key_store::count_kv_reads;

// verifies: P21
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

// verifies: P21
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
    group.update_consent_state(ConsentState::Denied)?;
    group.set_notifications(NotificationOverride::Enabled)?;
    group
        .update_conversation_message_disappearing_settings(
            xmtp_mls_common::group_mutable_metadata::MessageDisappearingSettings::new(1, 2),
        )
        .await?;

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

// verifies: P22
#[xmtp_common::test(unwrap_try = true)]
async fn test_last_activity_matches_list_order() {
    tester!(alix);
    let empty = alix.create_group(None, None)?;
    let first = alix.create_group(None, None)?;
    let second = alix.create_group(None, None)?;
    first
        .send_message(b"first", SendMessageOpts::default())
        .await?;
    second
        .send_message(b"second", SendMessageOpts::default())
        .await?;

    assert_eq!(empty.last_activity_ns(None)?, empty.created_at_ns);
    let ordered = alix.context.db().fetch_conversation_list(GroupQueryArgs {
        order_by: Some(GroupQueryOrderBy::LastActivity),
        ..Default::default()
    })?;
    let expected = [&empty, &first, &second];
    for group in expected {
        let row = ordered.iter().find(|row| row.id == group.group_id).unwrap();
        assert_eq!(
            group.last_activity_ns(None)?,
            row.sent_at_ns.unwrap_or(row.created_at_ns)
        );
    }
    for pair in ordered.windows(2) {
        assert!(
            pair[0].sent_at_ns.unwrap_or(pair[0].created_at_ns)
                >= pair[1].sent_at_ns.unwrap_or(pair[1].created_at_ns)
        );
    }
}

// verifies: P22
#[xmtp_common::test(unwrap_try = true)]
async fn test_last_activity_custom_content_types() {
    use xmtp_content_types::{ContentCodec, encoded_content_to_bytes, text::TextCodec};

    tester!(alix);
    let group = alix.create_group(None, None)?;
    group
        .send_message(b"plain", SendMessageOpts::default())
        .await?;
    let plain = group.last_activity_ns(None)?;
    let typed = encoded_content_to_bytes(TextCodec::encode("typed".into())?);
    group
        .send_message(&typed, SendMessageOpts::default())
        .await?;
    let all = group.last_activity_ns(None)?;
    assert!(all >= plain);
    assert_eq!(group.last_activity_ns(Some(&[]))?, group.created_at_ns);
    assert_eq!(group.last_activity_ns(Some(&[ContentType::Text]))?, all);
    assert_eq!(
        group.last_activity_ns(Some(&[ContentType::Unknown]))?,
        plain
    );
}
