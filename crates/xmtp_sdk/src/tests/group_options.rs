use super::*;

/// Group and DM create options reach the conversation state, the immutable
/// group fields read no database row, the debug, capability and message
/// list options map to the core, and the commit log fork status maps each
/// core value to its own variant.
#[xmtp_common::test(unwrap_try = true)]
async fn create_options_reach_state_and_immutable_fields_read_no_rows() {
    use crate::{
        CommitLogForkStatus, CreateDmOptions, CreateGroupOptions, DisappearingSettings,
        GroupPermissionMode, GroupPolicyType, ListMessagesOptions, MessageOrder, Timestamp,
    };

    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let settings = DisappearingSettings {
        from: Timestamp(xmtp_common::time::now_ns()),
        retention_ns: 2_000_000_000,
    };
    let group = alix
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
    let state = group.state().await?;
    assert_eq!(state.name, "Group Name");
    assert!(matches!(
        state.common.commit_log_fork_status,
        CommitLogForkStatus::Unknown
    ));
    assert_eq!(state.image_url, "url");
    assert_eq!(state.description, "group description");
    assert!(matches!(
        state.permissions.policy_type,
        GroupPolicyType::AdminOnly
    ));
    assert!(state.common.is_disappearing_enabled);
    let stored = state.common.disappearing_settings.expect("settings");
    assert_eq!(stored.from, settings.from);
    assert_eq!(stored.retention_ns, settings.retention_ns);
    let (_, immutable_reads, _) = xmtp_db::count_sql_queries(|| {
        assert_eq!(group.creator_inbox_id(), Some(alix.inbox_id()));
        assert_eq!(group.added_by_inbox_id(), Some(alix.inbox_id()));
        assert!(group.is_creator());
        assert!(!group.topic().is_empty());
    });
    assert_eq!(immutable_reads, 0, "immutable fields read the database");

    // A zero start time keeps the settings but leaves disappearing off.
    let all_members = alix
        .conversations()
        .create_group(
            vec![],
            Some(CreateGroupOptions {
                permissions: Some(GroupPermissionMode::AllMembers),
                disappearing: Some(DisappearingSettings {
                    from: Timestamp(0),
                    retention_ns: 5,
                }),
                ..Default::default()
            }),
        )
        .await?;
    let state = all_members.state().await?;
    assert!(matches!(
        state.permissions.policy_type,
        GroupPolicyType::AllMembers
    ));
    assert!(!state.common.is_disappearing_enabled);
    assert_eq!(
        state
            .common
            .disappearing_settings
            .expect("zero settings")
            .retention_ns,
        5
    );
    let debug = all_members.debug_info().await?;
    assert!(!debug.cursor.is_empty());
    let capabilities = all_members.membership_capabilities().await?;
    assert!(capabilities.members.iter().any(|member| {
        member.inbox_id == alix.inbox_id()
            && member
                .installations
                .iter()
                .any(|installation| installation.is_own)
    }));
    let first = all_members.send_text("first".into(), None).await?;
    let second = all_members.send_text("second".into(), None).await?;
    let newest = all_members
        .messages(Some(ListMessagesOptions {
            limit: Some(1),
            direction: Some(MessageOrder::Descending),
            ..Default::default()
        }))
        .await?;
    assert_eq!(newest.len(), 1);
    assert_eq!(newest[0].0.id, second);
    assert_ne!(first, second);
    let text_type = crate::encode_text("sample".into())?.r#type;
    let without_text = all_members
        .messages(Some(ListMessagesOptions {
            exclude_content_types: Some(vec![text_type]),
            ..Default::default()
        }))
        .await?;
    assert!(
        without_text
            .iter()
            .all(|message| message.0.id != first && message.0.id != second)
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
    assert!(matches!(
        dm_state.commit_log_fork_status,
        CommitLogForkStatus::Unknown
    ));
    // A known fork status maps to its own variant.
    let mut snapshot = dm.inner.state_snapshot()?;
    snapshot.commit_log_fork_status = Some(true);
    assert!(matches!(
        crate::ConversationState::from_snapshot(&snapshot).commit_log_fork_status,
        CommitLogForkStatus::Forked
    ));
    snapshot.commit_log_fork_status = Some(false);
    assert!(matches!(
        crate::ConversationState::from_snapshot(&snapshot).commit_log_fork_status,
        CommitLogForkStatus::NotForked
    ));
    assert_eq!(
        dm_state
            .disappearing_settings
            .expect("DM settings")
            .retention_ns,
        2_000_000_000
    );
    let groups = alix.conversations().list_groups(None).await?;
    let group_ids = groups
        .iter()
        .map(|group| group.id())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(groups.len(), 2, "list_groups returns only the groups");
    assert_eq!(
        group_ids,
        [group.id(), all_members.id()].into_iter().collect(),
        "list_groups returns only the groups"
    );
    let dms = alix.conversations().list_dms(None).await?;
    assert_eq!(
        dms.iter().map(|dm| dm.id()).collect::<Vec<_>>(),
        [dm.id()],
        "list_dms returns only the DM"
    );
    alix.end().await?;
    bo.end().await?;
}

// verifies: DMS-007
#[xmtp_common::test(unwrap_try = true)]
async fn duplicate_dm_message_actions_keep_typed_results() {
    use crate::{Reaction, ReactionAction, ReactionSchema};

    let a = Client::create(crate::generate_local_signer().await, options()).await?;
    let b = Client::create(crate::generate_local_signer().await, options()).await?;
    let first_dm = a.conversations().create_dm(b.inbox_id(), None).await?;
    let first = first_dm.send_text("first duplicate".into(), None).await?;
    let second_dm = b.conversations().create_dm(a.inbox_id(), None).await?;
    let second = second_dm.send_text("second duplicate".into(), None).await?;
    b.conversations().sync_all(None).await?;
    a.conversations().sync_all(None).await?;
    let a_thread = a
        .conversations()
        .get_dm_by_inbox_id(b.inbox_id())
        .await?
        .expect("A thread");
    let b_thread = b
        .conversations()
        .get_dm_by_inbox_id(a.inbox_id())
        .await?
        .expect("B thread");
    assert_eq!(a_thread.id(), b_thread.id());
    let thread_messages = a_thread.messages(None).await?;
    assert!(thread_messages.iter().any(|message| message.0.id == first));
    assert!(thread_messages.iter().any(|message| message.0.id == second));

    let mut inactive = None;
    for id in [first.clone(), second.clone()] {
        let bytes = id.to_bytes()?;
        if let Some((stored, stitched)) = a.inner.message_with_group(&bytes).await?
            && stored.group_id != stitched.group_id
        {
            inactive = Some(id);
            break;
        }
    }
    let id = inactive.expect("one duplicate DM must be inactive");
    let owner = if id == first { &a } else { &b };
    let stored = owner.inner.message(id.to_bytes()?)?;
    assert_eq!(stored.sender_inbox_id, owner.inbox_id().into_checked()?);
    let active_id = if id == first { &second } else { &first };
    let active_group_id = owner.inner.message(active_id.to_bytes()?)?.group_id;
    let crate::Conversation::Dm { dm: resolved_dm } = owner
        .conversations()
        .get_by_id(stored.group_id.into())
        .await?
        .expect("inactive DM resolves to the active DM")
    else {
        panic!("expected a DM");
    };
    assert_eq!(resolved_dm.id(), active_group_id.into());
    let peer_message = active_id.clone();
    owner.conversations().sync_all(None).await?;
    let crate::Conversation::Dm { dm: active_dm } = owner
        .conversations()
        .get_by_id(active_group_id.into())
        .await?
        .expect("active duplicate DM")
    else {
        panic!("expected a DM");
    };
    active_dm
        .send_text("keep other duplicate active".into(), None)
        .await?;
    for message_id in [&id] {
        let bytes = message_id.to_bytes()?;
        let (stored, winner) = owner
            .inner
            .message_with_group(&bytes)
            .await?
            .expect("message");
        assert_ne!(stored.group_id, winner.group_id);
    }
    let reaction = owner
        .conversations()
        .react_to_message(
            id.clone(),
            Reaction {
                content: "👍".into(),
                action: ReactionAction::Added,
                schema: ReactionSchema::Unicode,
            },
            None,
        )
        .await?;
    let reply = owner
        .conversations()
        .reply_to_message(id.clone(), crate::encode_text("reply".into())?, None)
        .await?;
    assert_ne!(reaction, reply);
    let peer_error = owner
        .conversations()
        .delete_message(peer_message)
        .await
        .unwrap_err();
    assert!(
        matches!(&peer_error, crate::XmtpError::PermissionDenied(details) if details.message.contains("not your message")),
        "{peer_error:?}"
    );
    assert_ne!(owner.conversations().delete_message(id.clone()).await?, id);
    a.end().await?;
    b.end().await?;
}

/// Clearing the disappearing settings (`None`) stores zero values and turns
/// disappearing off, for the sender and for a member after sync.
#[xmtp_common::test(unwrap_try = true)]
async fn cleared_disappearing_settings_read_back_as_zero_for_all_members() {
    use crate::{Conversation, ConversationState, DisappearingSettings, Timestamp};

    fn settings(state: &ConversationState) -> Option<(i64, i64)> {
        state
            .disappearing_settings
            .as_ref()
            .map(|settings| (settings.from.0, settings.retention_ns))
    }

    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    let from = xmtp_common::time::now_ns();
    group
        .update_disappearing_settings(Some(DisappearingSettings {
            from: Timestamp(from),
            retention_ns: xmtp_common::NS_IN_MIN,
        }))
        .await?;
    bo.conversations().sync().await?;
    let Some(Conversation::Group { group: bo_group }) =
        bo.conversations().get_by_id(group.id()).await?
    else {
        panic!("bo must receive the group");
    };
    bo_group.sync().await?;
    for state in [group.state().await?.common, bo_group.state().await?.common] {
        assert!(state.is_disappearing_enabled);
        assert_eq!(settings(&state), Some((from, xmtp_common::NS_IN_MIN)));
    }

    group.update_disappearing_settings(None).await?;
    bo_group.sync().await?;
    for state in [group.state().await?.common, bo_group.state().await?.common] {
        assert!(!state.is_disappearing_enabled);
        assert_eq!(settings(&state), Some((0, 0)));
    }
    alix.end().await?;
    bo.end().await?;
}

/// The façade description, image URL and super-admin mutations change the
/// matching group field for the sender and for a member after sync.
#[xmtp_common::test(unwrap_try = true)]
async fn group_mutations_change_only_their_own_field_for_all_members() {
    use crate::{Conversation, GroupState};

    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(
            vec![bo.inbox_id()],
            Some(crate::CreateGroupOptions {
                image_url: Some("https://example.com/first".into()),
                description: Some("first description".into()),
                ..Default::default()
            }),
        )
        .await?;
    group.update_description("next description".into()).await?;
    group
        .update_image_url("https://example.com/next".into())
        .await?;
    group.add_super_admin(bo.inbox_id()).await?;
    bo.conversations().sync().await?;
    let Some(Conversation::Group { group: bo_group }) =
        bo.conversations().get_by_id(group.id()).await?
    else {
        panic!("bo must receive the group");
    };
    bo_group.sync().await?;
    let is_super_admin = |state: &GroupState| state.super_admins.contains(&bo.inbox_id());
    for state in [group.state().await?, bo_group.state().await?] {
        assert_eq!(state.description, "next description");
        assert_eq!(state.image_url, "https://example.com/next");
        assert!(is_super_admin(&state), "{:?}", state.super_admins);
        assert!(!state.admins.contains(&bo.inbox_id()), "{:?}", state.admins);
    }

    group.remove_super_admin(bo.inbox_id()).await?;
    bo_group.sync().await?;
    for state in [group.state().await?, bo_group.state().await?] {
        assert!(!is_super_admin(&state), "{:?}", state.super_admins);
    }
    alix.end().await?;
    bo.end().await?;
}

/// An optimistic group keeps its create options locally, and a member sees
/// them after the group is published and the member is added.
#[xmtp_common::test(unwrap_try = true)]
async fn optimistic_group_keeps_create_options_after_publish() {
    use crate::{Conversation, CreateGroupOptions};

    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group_optimistic(Some(CreateGroupOptions {
            name: Some("optimistic".into()),
            description: Some("optimistic description".into()),
            ..Default::default()
        }))
        .await?;
    let state = group.state().await?;
    assert_eq!(state.name, "optimistic");
    assert_eq!(state.description, "optimistic description");

    group.publish_messages().await?;
    group.add_members(vec![bo.inbox_id()]).await?;
    bo.conversations().sync().await?;
    let Some(Conversation::Group { group: bo_group }) =
        bo.conversations().get_by_id(group.id()).await?
    else {
        panic!("bo must receive the optimistic group");
    };
    let state = bo_group.state().await?;
    assert_eq!(state.name, "optimistic");
    assert_eq!(state.description, "optimistic description");
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn remove_members_rejects_account_address_and_keeps_nonmember_noop() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let nonmember = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let before = group
        .members()
        .await?
        .into_iter()
        .map(|member| member.inbox_id)
        .collect::<std::collections::HashSet<_>>();
    for prefix in ["0x", "0X"] {
        let address = format!("{prefix}{}", "a".repeat(40));
        let error = group
            .remove_members(vec![crate::InboxId::unchecked(address)])
            .await
            .expect_err("account address must not be accepted as an inbox ID");
        let crate::XmtpError::InvalidArgument(details) = error else {
            panic!("expected InvalidArgument, got {error:?}");
        };
        assert_eq!(details.code, "InvalidArgument");
        assert!(matches!(details.category, crate::ErrorCategory::Input));
        assert!(!details.retryable);
    }
    group.remove_members(vec![nonmember.inbox_id()]).await?;
    let after = group
        .members()
        .await?
        .into_iter()
        .map(|member| member.inbox_id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(after, before);
    client.end().await?;
    nonmember.end().await?;
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

/// The façade reads conversation consent from the DM state and writes it with
/// `update_consent_state`. The creator starts allowed, the peer that joined from
/// a Welcome starts unknown, and each update is read back from the DM state and
/// from the conversation consent record.
// verifies: CONS-020, CONS-041
#[xmtp_common::test(unwrap_try = true)]
async fn dm_consent_is_read_and_updated_through_the_dm() {
    use crate::{ConsentEntity, ConsentState};
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let alix_dm = alix.conversations().create_dm(bo.inbox_id(), None).await?;
    assert!(matches!(
        alix_dm.state().await?.consent_state,
        ConsentState::Allowed
    ));
    bo.conversations().sync_all(None).await?;
    let bo_dm = bo
        .conversations()
        .get_dm_by_inbox_id(alix.inbox_id())
        .await?
        .expect("peer DM");
    assert!(matches!(
        bo_dm.state().await?.consent_state,
        ConsentState::Unknown
    ));

    alix_dm.update_consent_state(ConsentState::Denied).await?;
    assert!(matches!(
        alix_dm.state().await?.consent_state,
        ConsentState::Denied
    ));
    assert!(matches!(
        alix.preferences()
            .consent_state(ConsentEntity::Conversation {
                conversation_id: alix_dm.id(),
            })
            .await?,
        ConsentState::Denied
    ));

    bo_dm.update_consent_state(ConsentState::Allowed).await?;
    assert!(matches!(
        bo_dm.state().await?.consent_state,
        ConsentState::Allowed
    ));
    assert!(matches!(
        bo.preferences()
            .consent_state(ConsentEntity::Conversation {
                conversation_id: bo_dm.id(),
            })
            .await?,
        ConsentState::Allowed
    ));
    alix.end().await?;
    bo.end().await?;
}

/// The group state reports each membership state: the creator is allowed, an
/// invitee is pending after the welcome, and an invitee that requests removal
/// is pending removal.
#[xmtp_common::test(unwrap_try = true)]
async fn group_state_reports_membership_state() {
    use crate::{Conversation, MembershipState};

    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    let state = group.state().await?.membership_state;
    assert!(matches!(state, MembershipState::Allowed), "{state:?}");

    bo.conversations().sync().await?;
    let Some(Conversation::Group { group: bo_group }) =
        bo.conversations().get_by_id(group.id()).await?
    else {
        panic!("bo must receive the group");
    };
    let state = bo_group.state().await?.membership_state;
    assert!(matches!(state, MembershipState::Pending), "{state:?}");

    bo_group.sync().await?;
    bo_group.request_removal().await?;
    let state = bo_group.state().await?.membership_state;
    assert!(matches!(state, MembershipState::PendingRemove), "{state:?}");

    alix.end().await?;
    bo.end().await?;
}
