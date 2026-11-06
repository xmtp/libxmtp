//! Incoming controller tests: group selection.

use super::*;

/// Store a Restored archive placeholder for a group whose Welcome is not yet processed.
fn insert_restored_placeholder<C: XmtpSharedContext>(
    context: &C,
    group_id: &GroupId,
    conversation_type: ConversationType,
) {
    crate::groups::MlsGroup::insert(
        context,
        Some(group_id.as_slice()),
        GroupMembershipState::Restored,
        conversation_type,
        crate::groups::group_permissions::PolicySet::default(),
        xmtp_mls_group_state::group::GroupMetadataOptions::default(),
        None,
        false,
    )
    .unwrap();
}

fn membership<C: XmtpSharedContext>(context: &C, group_id: &GroupId) -> GroupMembershipState {
    context
        .db()
        .find_group(group_id)
        .unwrap()
        .unwrap()
        .membership_state
}

// verifies: PROC-051
#[rstest::rstest]
#[case::topics(0)]
#[case::groups(1)]
#[case::barrier(2)]
#[xmtp_common::test(unwrap_try = true)]
async fn a_selected_missing_group_retires_after_import_and_returns_after_welcome(
    #[case] kind: usize,
) {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await
        .unwrap();
    let topic = Topic::new_group_message(group.group_id);
    let mut controller = controller(bo.context.clone());
    let scope = match kind {
        0 => IncomingScope::Topics(vec![topic.clone()]),
        1 => IncomingScope::Groups(vec![group.group_id]),
        2 => IncomingScope::Barrier {
            targets: [(topic.clone(), Cursor(0))].into(),
            deadline: Instant::now() + Duration::from_secs(60),
            receive_policy: IncomingReceivePolicy::ImmediateQuery,
        },
        _ => unreachable!(),
    };
    controller.command(Command::Acquire { id: 1, scope });
    controller.reconcile().unwrap();
    assert!(controller.interested().contains(&topic));
    controller.reconcile().unwrap();
    assert!(controller.interested().contains(&topic));

    insert_restored_placeholder(&bo.context, &group.group_id, ConversationType::Group);
    controller.reconcile().unwrap();
    assert!(
        !controller.interested().contains(&topic),
        "the imported placeholder still has network interest"
    );

    xmtp_common::wait_for_eq(
        || async {
            bo.sync_welcomes().await.unwrap();
            bo.group(&group.group_id).unwrap().is_active().unwrap()
        },
        true,
    )
    .await
    .unwrap();
    controller.reconcile().unwrap();
    assert!(controller.interested().contains(&topic));
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn only_selected_missing_groups_add_repeated_lookup_cost() {
    use xmtp_db::database::count_sql_queries;
    let alix = EngineTestPeer::new().await;
    let mut controller = controller(alix.context.clone());
    let (_, baseline, _) = count_sql_queries(|| controller.reconcile().unwrap());
    let mut topics = Vec::new();
    for _ in 0..20 {
        let id = GroupId::generate();
        insert_restored_placeholder(&alix.context, &id, ConversationType::Group);
        set_membership_value(&alix.context, &id, GroupMembershipState::Allowed as i32);
        topics.push(Topic::new_group_message(id));
    }
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::Topics(topics),
    });
    controller.reconcile()?;
    for _ in 0..3 {
        let (_, queries, _) = count_sql_queries(|| controller.reconcile().unwrap());
        assert_eq!(queries, baseline, "stored groups added repeat lookups");
    }

    let missing = GroupId::generate();
    let topic = Topic::new_group_message(missing);
    controller.command(Command::Acquire {
        id: 2,
        scope: IncomingScope::Topics(vec![topic.clone()]),
    });
    controller.reconcile()?;
    let (_, queries, _) = count_sql_queries(|| controller.reconcile().unwrap());
    assert_eq!(
        queries,
        baseline + 1,
        "a selected missing group was not checked again"
    );
    assert!(controller.interested().contains(&topic));

    controller.command(Command::Release(2));
    let (_, queries, _) = count_sql_queries(|| controller.reconcile().unwrap());
    assert_eq!(
        queries, baseline,
        "a released missing group still had lookup cost"
    );

    controller.command(Command::Acquire {
        id: 2,
        scope: IncomingScope::Topics(vec![topic.clone()]),
    });
    controller.reconcile()?;
    insert_restored_placeholder(&alix.context, &missing, ConversationType::Group);
    set_membership_value(
        &alix.context,
        &missing,
        GroupMembershipState::Allowed as i32,
    );
    controller.reconcile()?;
    assert!(controller.interested().contains(&topic));
    let (_, queries, _) = count_sql_queries(|| controller.reconcile().unwrap());
    assert_eq!(
        queries, baseline,
        "a stored group remained on the missing-row check path"
    );
}

// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn a_missing_group_recheck_retries_after_a_storage_error() {
    let alix = EngineTestPeer::new().await;
    let id = GroupId::generate();
    let topic = Topic::new_group_message(id);
    let mut controller = controller(alix.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::Topics(vec![topic.clone()]),
    });
    controller.reconcile()?;
    insert_restored_placeholder(&alix.context, &id, ConversationType::Group);
    set_membership_value(&alix.context, &id, UNDECODABLE);
    assert!(matches!(
        controller.reconcile(),
        Err(IncomingError::Storage(_))
    ));

    set_membership_value(&alix.context, &id, GroupMembershipState::Restored as i32);
    controller.reconcile()?;
    assert!(!controller.interested().contains(&topic));
}

// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn all_groups_discovery_skips_a_restored_group_until_its_welcome() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let joined = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let restored = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    insert_restored_placeholder(&bo.context, &restored.group_id, ConversationType::Group);
    let restored_topic = Topic::new_group_message(restored.group_id);
    let joined_topic = Topic::new_group_message(joined.group_id);

    // A new controller is the resubscribe path after a restart.
    let mut controller = controller(bo.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::AllGroups,
    });
    controller.reconcile()?;
    assert!(controller.interested().contains(&joined_topic));
    assert!(!controller.interested().contains(&restored_topic));

    bo.sync_welcomes().await?;
    assert_ne!(
        membership(&bo.context, &restored.group_id),
        GroupMembershipState::Restored
    );
    controller.reconcile()?;
    assert!(controller.interested().contains(&restored_topic));
}

/// A scope that selects a Restored group does not receive its topic until a Welcome
/// activates the group.
async fn scope_waits_for_the_welcome(scope: impl Fn(&Topic, &Topic) -> IncomingScope) {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let restored = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await
        .unwrap();
    insert_restored_placeholder(&bo.context, &restored.group_id, ConversationType::Group);
    let topic = Topic::new_group_message(restored.group_id);
    let welcome = Topic::new_welcome_message(bo.context.installation_id());

    let mut controller = controller(bo.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: scope(&topic, &welcome),
    });
    controller.reconcile().unwrap();
    assert!(!controller.interested().contains(&topic));

    bo.sync_welcomes().await.unwrap();
    assert_ne!(
        membership(&bo.context, &restored.group_id),
        GroupMembershipState::Restored
    );
    controller.reconcile().unwrap();
    assert!(controller.interested().contains(&topic));
}

// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn an_explicit_groups_scope_skips_a_restored_group_until_its_welcome() {
    scope_waits_for_the_welcome(|topic, _| {
        IncomingScope::Groups(vec![GroupId::try_from(topic.identifier()).unwrap()])
    })
    .await;
}

// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn a_topics_scope_skips_a_restored_group_until_its_welcome() {
    scope_waits_for_the_welcome(|topic, welcome| {
        IncomingScope::Topics(vec![topic.clone(), welcome.clone()])
    })
    .await;
}

// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn a_barrier_scope_skips_a_restored_group_until_its_welcome() {
    scope_waits_for_the_welcome(|topic, _| IncomingScope::Barrier {
        targets: [(topic.clone(), Cursor(0))].into(),
        deadline: Instant::now() + Duration::from_secs(60),
        receive_policy: IncomingReceivePolicy::ImmediateQuery,
    })
    .await;
}

// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn a_device_sync_scope_skips_a_restored_sync_group() {
    let alix = EngineTestPeer::new().await;
    let group_id = GroupId::generate();
    insert_restored_placeholder(&alix.context, &group_id, ConversationType::Sync);
    let topic = Topic::new_group_message(group_id);
    let mut controller = controller(alix.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::DeviceSyncGroups,
    });
    controller.reconcile()?;
    // The sync-group query leaves out Restored rows, so the scope never
    // selects the topic and holds no interest in it.
    assert!(!controller.scopes[&1].topics.contains(&topic));
    assert!(!controller.interested().contains(&topic));
    assert!(
        controller
            .interested()
            .contains(&Topic::new_welcome_message(alix.context.installation_id()))
    );
}

// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn releasing_a_scope_forgets_its_restored_groups_until_selected_again() {
    let alix = EngineTestPeer::new().await;
    let group_id = GroupId::generate();
    insert_restored_placeholder(&alix.context, &group_id, ConversationType::Group);
    let topic = Topic::new_group_message(group_id);
    let mut controller = controller(alix.context.clone());
    let scope = || Command::Acquire {
        id: 1,
        scope: IncomingScope::Topics(vec![topic.clone()]),
    };
    controller.command(scope());
    controller.reconcile()?;
    assert!(controller.is_retired(&topic));

    controller.command(Command::Release(1));
    controller.reconcile()?;
    assert!(!controller.topics.contains_key(&topic));

    controller.command(scope());
    controller.reconcile()?;
    assert!(!controller.interested().contains(&topic));
}

/// Queue `count` Acquire commands the way callers send them. Hold the leases:
/// a dropped lease queues its Release.
fn acquire_scopes(coordinator: &Arc<IncomingCoordinator>, count: usize) -> Vec<IncomingLease> {
    (0..count)
        .map(|_| coordinator.acquire(IncomingScope::Topics(Vec::new())))
        .collect()
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_burst_of_commands_is_applied_before_one_pass() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let _leases = acquire_scopes(&coordinator, 100);
    // The run loop receives one command, applies the burst, then runs one pass.
    let first = controller.commands.try_recv()?;
    controller.command_burst(first);
    assert_eq!(controller.scopes.len(), 100);
    assert!(controller.commands.try_recv().is_err());
}

/// Write a raw membership state. A value outside the enum makes every lookup of
/// this group fail to decode, which stands in for a storage error on one group.
fn set_membership_value<C: XmtpSharedContext>(context: &C, group_id: &GroupId, value: i32) {
    use xmtp_db::{ConnectionExt, diesel::RunQueryDsl};
    let changed = context
        .db()
        .raw_query(|conn| {
            xmtp_db::diesel::sql_query(format!(
                "UPDATE groups SET membership_state = {value} WHERE id = X'{}'",
                hex::encode(group_id)
            ))
            .execute(conn)
        })
        .unwrap();
    assert_eq!(changed, 1);
}

const UNDECODABLE: i32 = 99;

// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn a_failed_selection_check_selects_nothing_until_it_succeeds() {
    let alix = EngineTestPeer::new().await;
    let failing = GroupId::generate();
    insert_restored_placeholder(&alix.context, &failing, ConversationType::Group);
    set_membership_value(&alix.context, &failing, UNDECODABLE);
    assert!(alix.context.db().find_group(&failing).is_err());
    let kept = alix.create_group(None, None)?;
    let kept = Topic::new_group_message(kept.group_id);
    let topic = Topic::new_group_message(failing);
    let mut controller = controller(alix.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::Topics(vec![kept.clone()]),
    });
    pass(&mut controller);
    assert!(controller.transport.requested.contains(&kept));

    // The failed check fails the pass. It opens no new interest and runs again.
    controller.command(Command::Acquire {
        id: 2,
        scope: IncomingScope::Topics(vec![topic.clone()]),
    });
    for _ in 0..2 {
        pass(&mut controller);
        assert!(controller.storage_error.is_some());
        assert!(!controller.transport.requested.contains(&topic));
        assert!(controller.transport.requested.contains(&kept));
    }

    set_membership_value(
        &alix.context,
        &failing,
        GroupMembershipState::Restored as i32,
    );
    pass(&mut controller);
    assert!(controller.storage_error.is_none());
    assert!(controller.is_retired(&topic));
    assert!(!controller.transport.requested.contains(&topic));
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_command_burst_applies_at_most_the_cap_per_step() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let _leases = acquire_scopes(&coordinator, MAX_COMMANDS_PER_PASS + 44);

    let first = controller.commands.try_recv()?;
    controller.command_burst(first);
    assert_eq!(controller.scopes.len(), MAX_COMMANDS_PER_PASS);

    // The remainder applies on the next step.
    let first = controller.commands.try_recv()?;
    controller.command_burst(first);
    assert_eq!(controller.scopes.len(), MAX_COMMANDS_PER_PASS + 44);
    assert!(controller.commands.try_recv().is_err());
}

// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn a_failed_reconcile_opens_no_new_network_interest() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let kept = Topic::new_group_message(group.group_id);
    let added = Topic::new_group_message(GroupId::generate());
    let mut controller = controller(alix.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::Topics(vec![kept.clone()]),
    });
    pass(&mut controller);
    assert!(controller.transport.requested.contains(&kept));

    // The pending-capacity step at the end of reconcile fails after the new
    // scope's topics are already selected.
    run_raw_sql(
        &alix.context,
        "ALTER TABLE incoming_envelopes RENAME TO incoming_envelopes_hidden",
    );
    controller.command(Command::Acquire {
        id: 2,
        scope: IncomingScope::Topics(vec![added.clone()]),
    });
    pass(&mut controller);
    assert!(controller.storage_error.is_some());
    assert!(controller.transport.requested.contains(&kept));
    assert!(!controller.transport.requested.contains(&added));

    run_raw_sql(
        &alix.context,
        "ALTER TABLE incoming_envelopes_hidden RENAME TO incoming_envelopes",
    );
    pass(&mut controller);
    assert!(controller.storage_error.is_none());
    assert!(controller.transport.requested.contains(&added));
}
