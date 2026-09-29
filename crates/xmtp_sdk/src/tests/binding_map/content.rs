use super::*;

#[xmtp_common::test(unwrap_try = true)]
fn facade_extended_content_records_keep_nested_fields() {
    use prost::Message as _;
    use xmtp_content_types::{
        ContentCodec,
        actions::{
            Action as CoreAction, ActionStyle as CoreActionStyle, Actions as CoreActions,
            ActionsCodec,
        },
        group_updated::GroupUpdatedCodec,
        intent::{Intent as CoreIntent, IntentCodec},
        leave_request::LeaveRequestCodec,
        text::TextCodec,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupUpdated as CoreGroupUpdated, content_types::LeaveRequest,
    };

    for text in [
        "",
        "Hello 👋 World 🌍! こんにちは 🎉",
        "Line 1\nLine 2\tTabbed\r\nWindows newline",
    ] {
        let encoded = crate::encode_text(text.into())?;
        let decoded = MessageContent::decode(
            xmtp_proto::xmtp::mls::message_contents::EncodedContent::from(encoded).encode_to_vec(),
        )?;
        assert!(matches!(decoded, MessageContent::Text(value) if value == text));
        assert!(
            matches!(MessageContent::decode(TextCodec::encode(text.into())?.encode_to_vec())?, MessageContent::Text(value) if value == text)
        );
    }
    assert!(MessageContent::decode(vec![0xff; 4]).is_err());

    let intent = CoreIntent {
        id: "intent-id".into(),
        action_id: "action-id".into(),
        metadata: Some(serde_json::from_value(
            serde_json::json!({"nested": {"value": 7}}),
        )?),
    };
    assert!(matches!(
        MessageContent::decode(IntentCodec::encode(intent)?.encode_to_vec())?,
        MessageContent::Intent(value)
            if value.id == "intent-id"
                && value.action_id == "action-id"
                && value.metadata_json.as_deref().is_some_and(|json| json.contains("nested"))
    ));
    let actions = CoreActions {
        id: "actions-id".into(),
        description: "choose".into(),
        actions: vec![CoreAction {
            id: "button".into(),
            label: "Confirm".into(),
            image_url: Some("https://example.org/icon".into()),
            style: Some(CoreActionStyle::Primary),
            expires_at: None,
        }],
        expires_at: None,
    };
    assert!(matches!(
        MessageContent::decode(ActionsCodec::encode(actions)?.encode_to_vec())?,
        MessageContent::Actions(value)
            if value.id == "actions-id"
                && value.description == "choose"
                && value.actions.len() == 1
                && value.actions[0].id == "button"
                && value.actions[0].label == "Confirm"
                && matches!(value.actions[0].style, Some(crate::ActionStyle::Primary))
    ));
    use xmtp_proto::xmtp::mls::message_contents::group_updated::{
        Inbox as ProtoInbox, MetadataFieldChange as ProtoFieldChange,
    };
    let update = CoreGroupUpdated {
        initiated_by_inbox_id: "inbox".into(),
        added_inboxes: vec![ProtoInbox {
            inbox_id: "added".into(),
        }],
        removed_inboxes: vec![ProtoInbox {
            inbox_id: "removed".into(),
        }],
        left_inboxes: vec![ProtoInbox {
            inbox_id: "left".into(),
        }],
        metadata_field_changes: vec![ProtoFieldChange {
            field_name: "name".into(),
            old_value: Some("old".into()),
            new_value: Some("new".into()),
        }],
        added_admin_inboxes: vec![ProtoInbox {
            inbox_id: "added-admin".into(),
        }],
        removed_admin_inboxes: vec![ProtoInbox {
            inbox_id: "removed-admin".into(),
        }],
        added_super_admin_inboxes: vec![ProtoInbox {
            inbox_id: "added-super".into(),
        }],
        removed_super_admin_inboxes: vec![ProtoInbox {
            inbox_id: "removed-super".into(),
        }],
    };
    let MessageContent::GroupUpdated(update) =
        MessageContent::decode(GroupUpdatedCodec::encode(update)?.encode_to_vec())?
    else {
        panic!("group update")
    };
    assert_eq!(update.initiated_by_inbox_id.checked()?, "inbox");
    assert_eq!(update.added_inboxes[0].checked()?, "added");
    assert_eq!(update.removed_inboxes[0].checked()?, "removed");
    assert_eq!(update.left_inboxes[0].checked()?, "left");
    assert_eq!(update.metadata_field_changes[0].field_name, "name");
    assert_eq!(
        update.metadata_field_changes[0].old_value.as_deref(),
        Some("old")
    );
    assert_eq!(
        update.metadata_field_changes[0].new_value.as_deref(),
        Some("new")
    );
    assert_eq!(update.added_admin_inboxes[0].checked()?, "added-admin");
    assert_eq!(update.removed_admin_inboxes[0].checked()?, "removed-admin");
    assert_eq!(
        update.added_super_admin_inboxes[0].checked()?,
        "added-super"
    );
    assert_eq!(
        update.removed_super_admin_inboxes[0].checked()?,
        "removed-super"
    );
    for note in [None, Some(b"leaving".to_vec())] {
        let encoded = LeaveRequestCodec::encode(LeaveRequest {
            authenticated_note: note.clone(),
        })?;
        assert!(matches!(
            MessageContent::decode(encoded.encode_to_vec())?,
            MessageContent::LeaveRequest(value) if value.authenticated_note == note
        ));
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn create_options_keep_disappearing_settings_and_preset_permissions() {
    use crate::{
        CreateDmOptions, CreateGroupOptions, DisappearingSettings, GroupPermissionMode,
        PermissionPolicy as Policy, Timestamp,
    };
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let settings = DisappearingSettings {
        from: Timestamp(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos() as i64,
        ),
        retention_ns: 2_000_000_000,
    };
    let group = alix
        .conversations()
        .create_group(
            vec![bo.inbox_id()],
            Some(CreateGroupOptions {
                permissions: Some(GroupPermissionMode::AllMembers),
                disappearing: Some(settings.clone()),
                ..Default::default()
            }),
        )
        .await?;
    let group_state = group.state().await?;
    assert!(group_state.common.is_disappearing_enabled);
    assert_eq!(
        group_state
            .common
            .disappearing_settings
            .expect("settings")
            .from,
        settings.from
    );
    assert!(matches!(
        group_state.permissions.policy_set.add_member,
        Policy::Allow
    ));
    assert!(matches!(
        group_state.permissions.policy_set.remove_member,
        Policy::Admin
    ));
    assert!(matches!(
        group_state.permissions.policy_set.add_admin,
        Policy::SuperAdmin
    ));
    assert!(matches!(
        group_state.permissions.policy_set.remove_admin,
        Policy::SuperAdmin
    ));
    assert!(matches!(
        group_state.permissions.policy_set.update_name,
        Policy::Allow
    ));
    assert!(matches!(
        group_state.permissions.policy_set.update_description,
        Policy::Allow
    ));
    assert!(matches!(
        group_state.permissions.policy_set.update_image,
        Policy::Allow
    ));
    assert!(matches!(
        group_state.permissions.policy_set.update_disappearing,
        Policy::Admin
    ));
    assert!(matches!(
        group_state.permissions.policy_set.update_app_data,
        Policy::Allow
    ));
    let admins_only = alix
        .conversations()
        .create_group(
            vec![bo.inbox_id()],
            Some(CreateGroupOptions {
                permissions: Some(GroupPermissionMode::AdminOnly),
                name: Some("Group Name".into()),
                image_url: Some("url".into()),
                description: Some("group description".into()),
                disappearing: Some(settings.clone()),
                ..Default::default()
            }),
        )
        .await?;
    let admin_state = admins_only.state().await?;
    assert!(admin_state.common.is_disappearing_enabled);
    assert_eq!(admin_state.name, "Group Name");
    assert_eq!(admin_state.image_url, "url");
    assert_eq!(admin_state.description, "group description");
    let policy = admin_state.permissions.policy_set;
    assert!(matches!(policy.add_member, Policy::Admin));
    assert!(matches!(policy.remove_member, Policy::Admin));
    assert!(matches!(policy.add_admin, Policy::SuperAdmin));
    assert!(matches!(policy.remove_admin, Policy::SuperAdmin));
    assert!(matches!(policy.update_name, Policy::Admin));
    assert!(matches!(policy.update_description, Policy::Admin));
    assert!(matches!(policy.update_image, Policy::Admin));
    assert!(matches!(policy.update_disappearing, Policy::Admin));
    assert!(matches!(policy.update_app_data, Policy::Admin));
    let zero_from = alix
        .conversations()
        .create_group(
            vec![bo.inbox_id()],
            Some(CreateGroupOptions {
                disappearing: Some(DisappearingSettings {
                    from: Timestamp(0),
                    retention_ns: 5,
                }),
                ..Default::default()
            }),
        )
        .await?;
    let zero_state = zero_from.state().await?;
    assert!(!zero_state.common.is_disappearing_enabled);
    assert_eq!(
        zero_state
            .common
            .disappearing_settings
            .expect("zero settings")
            .retention_ns,
        5
    );
    let dm = alix
        .conversations()
        .create_dm(
            bo.inbox_id(),
            Some(CreateDmOptions {
                disappearing: Some(settings),
            }),
        )
        .await?;
    let dm_state = dm.state().await?;
    assert!(dm_state.is_disappearing_enabled);
    assert_eq!(
        dm_state
            .disappearing_settings
            .expect("DM settings")
            .retention_ns,
        2_000_000_000
    );
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn can_message_changes_after_peer_registration() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo_signer = crate::generate_local_signer().await;
    let bo_identity = bo_signer.identity().await?;
    let before = alix.can_message(vec![bo_identity.clone()]).await?;
    assert_eq!(before.len(), 1);
    assert!(!before[&format!("ethereum:{}", bo_identity.identifier)]);
    let bo = Client::create(bo_signer, options()).await?;
    let after = alix.can_message(vec![bo_identity.clone()]).await?;
    assert_eq!(after.len(), 1);
    assert!(after[&format!("ethereum:{}", bo_identity.identifier)]);
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn member_consent_is_visible_in_group_member_record() {
    use crate::{ConsentEntity, ConsentRecord, ConsentState};
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    let entity = ConsentEntity::Inbox {
        inbox_id: bo.inbox_id(),
    };
    alix.preferences()
        .set_consent_states(vec![ConsentRecord {
            entity: entity.clone(),
            state: ConsentState::Allowed,
        }])
        .await?;
    assert!(matches!(
        alix.preferences().consent_state(entity).await?,
        ConsentState::Allowed
    ));
    let member = group
        .members()
        .await?
        .into_iter()
        .find(|member| member.inbox_id == bo.inbox_id())
        .expect("peer member");
    assert!(matches!(member.consent_state, ConsentState::Allowed));
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn storage_key_rejects_wrong_key_for_existing_database() {
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-binding-key-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let signer = crate::generate_local_signer().await;
    let mut first_options = options();
    first_options.storage = StorageOptions {
        location: StorageLocation::Path(path.to_string_lossy().into_owned()),
        encryption_key: Some(vec![7; 32]),
        ..Default::default()
    };
    let first = Client::create(signer.clone(), first_options.clone()).await?;
    first.end().await?;
    let second = Client::create(
        signer,
        ClientOptions {
            storage: StorageOptions {
                encryption_key: Some(vec![8; 32]),
                ..first_options.storage
            },
            ..first_options
        },
    )
    .await;
    assert!(second.is_err(), "a different database key must fail");
    std::fs::remove_file(path)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn passkey_signature_associates_identity_through_facade() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let passkey = xmtp_id::utils::passkey::PasskeyUser::new().await;
    let identity = PublicIdentity::from(passkey.get_identifier()?);
    let request = alix
        .unsafe_add_account_signature_request(identity.clone(), false)
        .await?;
    let UnverifiedSignature::Passkey(signature) = passkey.sign(&request.signature_text().await)?
    else {
        panic!("passkey fixture returned the wrong signature kind");
    };
    request
        .add_signature(Signature::Passkey {
            signature: signature.signature,
            public_key: signature.public_key,
            authenticator_data: signature.authenticator_data,
            client_data_json: signature.client_data_json,
        })
        .await?;
    alix.unsafe_apply_signature_request(request).await?;
    let state = alix.inbox_state(true).await?;
    assert!(
        state
            .identities
            .iter()
            .any(|value| value.identifier == identity.identifier
                && matches!(value.kind, PublicIdentityKind::Passkey))
    );
    alix.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn new_installation_can_find_existing_dm() {
    let signer = crate::generate_local_signer().await;
    let sync_options = ClientOptions {
        device_sync: true,
        ..options()
    };
    let first = Client::create(signer.clone(), sync_options.clone()).await?;
    let peer = Client::create(crate::generate_local_signer().await, options()).await?;
    let dm = first
        .conversations()
        .create_dm(peer.inbox_id(), None)
        .await?;
    first.conversations().sync().await?;
    peer.conversations().sync().await?;
    let second = Client::create(signer, sync_options).await?;
    assert!(second.conversations().list(None).await?.is_empty());
    dm.send_text("new installation delivery".into(), None)
        .await?;
    first.conversations().sync().await?;
    second.catch_up_to_live(None).await?;
    let found = second
        .conversations()
        .get_dm_by_inbox_id(peer.inbox_id())
        .await?
        .expect("new installation can find the DM");
    assert_eq!(found.id(), dm.id());
    assert_eq!(found.peer_inbox_id(), peer.inbox_id());
    first.end().await?;
    second.end().await?;
    peer.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn removed_member_does_not_receive_later_group_message() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    let bo_group = xmtp_common::time::timeout(Duration::from_secs(10), async {
        loop {
            bo.conversations().sync().await?;
            if let Some(found) = bo.conversations().get_by_id(group.id()).await? {
                break Ok::<_, XmtpError>(found);
            }
            xmtp_common::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await??;
    let crate::Conversation::Group { group: bo_group } = bo_group else {
        panic!("expected group")
    };
    group.remove_members(vec![bo.inbox_id()]).await?;
    group.send_text("only current members".into(), None).await?;
    xmtp_common::time::timeout(Duration::from_secs(10), async {
        loop {
            let _ = bo_group.sync().await;
            if !bo_group.state().await?.common.is_active {
                break Ok::<(), XmtpError>(());
            }
            xmtp_common::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await??;
    assert!(!bo_group.state().await?.common.is_active);
    assert!(!bo_group.messages(None).await?.iter().any(|message| {
        matches!(&message.0.content, MessageContent::Text(value) if value == "only current members")
    }));
    alix.end().await?;
    bo.end().await?;
}
