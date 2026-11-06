//! Incoming controller tests: interest.

use super::*;

// verifies: EVENT-027
#[xmtp_common::test(unwrap_try = true)]
async fn internal_interest_does_not_change_client_connection_state() {
    let alix = EngineTestPeer::new().await;
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ConnectionStateChanged]),
        Some(4),
    );
    let coordinator = IncomingCoordinator::for_context(&alix.context);
    let internal = coordinator.acquire(IncomingScope::DeviceSyncGroups);
    xmtp_common::task::yield_now().await;
    assert!(events.drain().is_empty());
    let app = coordinator.acquire_stream(IncomingScope::Topics(Vec::new()));
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConnectionStateChanged(change)), ..
        }] if change.previous == xmtp_events::ConnectionState::Closed
            && change.current == xmtp_events::ConnectionState::Connecting
    ));
    app.close();
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConnectionStateChanged(change)), ..
        }] if change.current == xmtp_events::ConnectionState::Closed
    ));
    internal.close();
    assert!(events.drain().is_empty());
}

// verifies: DMS-009
#[xmtp_common::test(unwrap_try = true)]
async fn one_dm_interest_reconciles_a_later_joined_duplicate() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let first = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await?;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let lease = coordinator.acquire_stream(IncomingScope::Groups(vec![first.group_id]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller.reconcile()?;
    let first_topic = Topic::new_group_message(first.group_id);
    assert!(controller.interested().contains(&first_topic));
    let welcome_topic = Topic::new_welcome_message(alix.context.installation_id());
    assert!(!controller.scopes[&lease.id].topics.contains(&welcome_topic));
    assert!(!controller.interested().contains(&welcome_topic));

    let second = bo
        .find_or_create_dm(alix.inbox_id().to_string(), None)
        .await?;
    assert_ne!(first.group_id, second.group_id);
    alix.sync_welcomes().await?;
    controller.reconcile()?;
    let second_topic = Topic::new_group_message(second.group_id);
    assert!(controller.scopes[&lease.id].topics.contains(&first_topic));
    assert!(controller.scopes[&lease.id].topics.contains(&second_topic));
    assert!(controller.interested().contains(&second_topic));
    assert!(!controller.interested().contains(&welcome_topic));
}

// verifies: DMS-009
#[rstest::rstest]
#[case::stored_before_reader(false)]
#[case::stored_after_reader(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn inferred_inactive_dm_does_not_request_welcome_recovery(#[case] stored_later: bool) {
    use xmtp_db::{Store, group::GroupMembershipState};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let first = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await
        .unwrap();
    assert!(first.is_active().unwrap());
    let mut sibling = alix
        .context
        .db()
        .find_group(&first.group_id)
        .unwrap()
        .unwrap();
    sibling.id = GroupId::generate();
    sibling.membership_state = GroupMembershipState::Restored;
    if !stored_later {
        sibling.store(&alix.context.db()).unwrap();
    }

    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let lease = coordinator.acquire_stream(IncomingScope::Groups(vec![first.group_id]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller.reconcile().unwrap();
    let first_topic = Topic::new_group_message(first.group_id);
    let sibling_topic = Topic::new_group_message(sibling.id);
    let welcome_topic = Topic::new_welcome_message(alix.context.installation_id());
    if stored_later {
        assert!(!controller.scopes[&lease.id].topics.contains(&sibling_topic));
        sibling.store(&alix.context.db()).unwrap();
        controller.reconcile().unwrap();
    }
    assert!(
        !MlsGroup::load_by_id(&alix.context, &sibling.id)
            .unwrap()
            .is_active()
            .unwrap()
    );

    for _ in 0..2 {
        assert!(controller.scopes[&lease.id].topics.contains(&first_topic));
        assert!(controller.scopes[&lease.id].topics.contains(&sibling_topic));
        assert!(controller.is_retired(&sibling_topic));
        assert!(!controller.scopes[&lease.id].topics.contains(&welcome_topic));
        assert!(!controller.interested().contains(&welcome_topic));
        assert!(!controller.read_queue.contains(&welcome_topic));
        controller.reconcile().unwrap();
    }

    // An explicitly selected inactive group keeps its existing recovery interest.
    lease.replace_scope(IncomingScope::Groups(vec![sibling.id]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller.reconcile().unwrap();
    assert!(controller.interested().contains(&welcome_topic));
    assert!(controller.read_queue.contains(&welcome_topic));

    lease.replace_scope(IncomingScope::Groups(vec![first.group_id]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller.reconcile().unwrap();
    assert!(controller.scopes[&lease.id].topics.contains(&sibling_topic));
    assert!(controller.is_retired(&sibling_topic));
    assert!(!controller.interested().contains(&welcome_topic));
    assert!(!controller.read_queue.contains(&welcome_topic));
}

// verifies: PROC-046
#[xmtp_common::test(unwrap_try = true)]
async fn all_group_network_interest_keeps_a_denied_conversation() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.update_consent_state(xmtp_db::consent_record::ConsentState::Denied)?;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let lease = coordinator.acquire_stream(IncomingScope::AllGroups);
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller.reconcile()?;
    let topic = Topic::new_group_message(group.group_id);
    assert!(controller.scopes[&lease.id].topics.contains(&topic));
    assert!(controller.interested().contains(&topic));
}

/// An unrelated inbox can fill the receiver's all-group scope with valid
/// invitations. Discovery reads the groups table again only after the stored
/// group set changes, not on every pass.
// verifies: PROC-046, PROC-015
#[xmtp_common::test(unwrap_try = true)]
async fn all_group_discovery_reruns_only_after_a_group_change() {
    let sender = EngineTestPeer::new().await;
    let receiver = EngineTestPeer::new().await;

    const INVITATIONS: usize = 12;
    let mut invited_topics = HashSet::new();
    for _ in 0..INVITATIONS {
        let group = sender.create_group(None, None)?;
        group.add_members(&[receiver.inbox_id()]).await?;
        invited_topics.insert(Topic::new_group_message(group.group_id));
    }
    receiver.sync_welcomes().await?;

    let (coordinator, mut controller) = coordinated_controller(receiver.context.clone());
    let lease = coordinator.acquire_stream(IncomingScope::AllGroups);
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller.reconcile()?;
    for topic in &invited_topics {
        let group_id = GroupId::try_from(topic.identifier())?;
        assert!(receiver.context.db().find_group(&group_id)?.is_some());
        assert!(controller.scopes[&lease.id].topics.contains(topic));
        assert!(controller.interested().contains(topic));
    }
    assert!(controller.scopes[&lease.id].topics.len() >= INVITATIONS);
    controller.reconcile()?;
    controller.reconcile()?;
    assert_eq!(
        controller.discovery_reads, 1,
        "an idle pass read every group"
    );

    let group = sender.create_group(None, None)?;
    group.add_members(&[receiver.inbox_id()]).await?;
    receiver.sync_welcomes().await?;
    controller.reconcile()?;
    assert_eq!(controller.discovery_reads, 2);
    let topic = Topic::new_group_message(group.group_id);
    assert!(controller.scopes[&lease.id].topics.contains(&topic));
    assert!(controller.interested().contains(&topic));
}

/// A stranger who invites and then removes the user leaves a retired group.
/// Its stored and MLS state are loaded again only after the group set changes.
// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn retired_groups_are_rechecked_only_after_a_group_change() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let mut groups = Vec::new();
    for _ in 0..3 {
        let group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        groups.push(group);
    }
    bo.sync_welcomes().await?;
    for group in &groups {
        group.remove_members(&[bo.inbox_id()]).await?;
        let bo_group = bo.group(&group.group_id)?;
        bo_group.sync().await?;
        assert!(!bo_group.is_active()?);
    }
    let topics: Vec<_> = groups
        .iter()
        .map(|group| Topic::new_group_message(group.group_id))
        .collect();
    let mut controller = controller(bo.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::Groups(groups.iter().map(|group| group.group_id).collect()),
    });
    controller.reconcile()?;
    assert!(topics.iter().all(|topic| controller.is_retired(topic)));
    controller.reconcile()?;
    controller.reconcile()?;
    assert_eq!(
        controller.retired_checks, 0,
        "an idle pass loaded a retired group"
    );

    groups[0].add_members(&[bo.inbox_id()]).await?;
    xmtp_common::wait_for_eq(
        || async {
            bo.sync_welcomes().await.unwrap();
            bo.group(&groups[0].group_id).unwrap().is_active().unwrap()
        },
        true,
    )
    .await?;
    controller.reconcile()?;
    assert_eq!(controller.retired_checks, 3);
    assert!(!controller.is_retired(&topics[0]));
    assert!(controller.is_retired(&topics[1]));
    assert!(controller.is_retired(&topics[2]));
}

/// Welcome publication order is not epoch order, so a rejoin Welcome can carry
/// a lower cursor than the one the group's row already holds. Installing it
/// still changes the group set, so the retired topic is checked again.
// verifies: PROC-051
#[xmtp_common::test(unwrap_try = true)]
async fn an_older_rejoin_welcome_unretires_the_group() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    group.remove_members(&[bo.inbox_id()]).await?;
    let bo_group = bo.group(&group.group_id)?;
    bo_group.sync().await?;
    assert!(!bo_group.is_active()?);
    // A later-published earlier Welcome already advanced the row's cursor.
    run_raw_sql(
        &bo.context,
        &format!(
            "UPDATE groups SET sequence_id = {} WHERE id = x'{}'",
            i64::MAX,
            hex::encode(group.group_id)
        ),
    );
    let topic = Topic::new_group_message(group.group_id);
    let mut controller = controller(bo.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::Groups(vec![group.group_id]),
    });
    controller.reconcile()?;
    assert!(controller.is_retired(&topic));

    group.add_members(&[bo.inbox_id()]).await?;
    xmtp_common::wait_for_eq(
        || async {
            bo.sync_welcomes().await.unwrap();
            bo.group(&group.group_id).unwrap().is_active().unwrap()
        },
        true,
    )
    .await?;
    controller.reconcile()?;
    assert!(!controller.is_retired(&topic));
    assert!(controller.interested().contains(&topic));
}

/// A scope opened while no other all-groups scope is open takes the topics of
/// the last discovery read when the group set has not changed since.
// verifies: PROC-015
#[xmtp_common::test(unwrap_try = true)]
async fn later_all_groups_scope_gets_cached_topics_at_an_unchanged_generation() {
    let alix = EngineTestPeer::new().await;
    let mut group_topics = HashSet::new();
    for _ in 0..12 {
        let group = alix.create_group(None, None)?;
        group_topics.insert(Topic::new_group_message(group.group_id));
    }
    let mut controller = controller(alix.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::AllGroups,
    });
    controller.reconcile()?;
    assert_eq!(controller.discovery_reads, 1);
    controller.command(Command::Release(1));
    controller.reconcile()?;

    controller.command(Command::Acquire {
        id: 2,
        scope: IncomingScope::AllGroups,
    });
    controller.reconcile()?;
    assert!(group_topics.is_subset(&controller.scopes[&2].topics));
    assert_eq!(controller.discovery_reads, 1);
}

/// Rebuilding the read queue costs one hash lookup per topic, not a scan of
/// the queue for each interested topic.
#[xmtp_common::test]
fn read_queue_merge_uses_one_lookup_per_topic() {
    use std::cell::Cell;
    std::thread_local! {
        static EQUALITY_CHECKS: Cell<usize> = const { Cell::new(0) };
    }
    #[derive(Clone, Debug)]
    struct Key(u64);
    impl std::hash::Hash for Key {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.0.hash(state);
        }
    }
    impl PartialEq for Key {
        fn eq(&self, other: &Self) -> bool {
            EQUALITY_CHECKS.with(|checks| checks.set(checks.get() + 1));
            self.0 == other.0
        }
    }
    impl Eq for Key {}

    const QUEUED: u64 = 2_000;
    let mut queue: VecDeque<_> = (0..QUEUED).rev().map(Key).collect();
    let interested: HashSet<_> = (0..=QUEUED).map(Key).collect();
    EQUALITY_CHECKS.with(|checks| checks.set(0));
    merge_read_queue(&mut queue, &interested, |key| key.0);
    let checks = EQUALITY_CHECKS.with(Cell::get);
    assert!(checks <= 16_004, "{checks} equality checks");
    let order: Vec<_> = queue.iter().map(|key| key.0).collect();
    let expected: Vec<_> = (0..QUEUED).rev().chain([QUEUED]).collect();
    assert_eq!(order, expected);
}

#[xmtp_common::test(unwrap_try = true)]
fn reopening_after_the_last_release_keeps_the_controller_alive() {
    let context = context();
    let (commands, receiver) = mpsc::unbounded_channel();
    let state = Arc::new(SharedState::default());
    let coordinator = Arc::new(IncomingCoordinator {
        commands,
        generations: AtomicU64::new(0),
        state: state.clone(),
    });
    *context.incoming_runtime().coordinator.lock() = Some(coordinator.clone());
    let mut controller = Controller::new(context, receiver, state);

    // The returned handle can exist before its first acquire command.
    assert!(!controller.stop_if_idle());
    let first = coordinator.acquire(IncomingScope::Topics(vec![]));
    let acquire = controller.commands.try_recv()?;
    controller.command(acquire);
    drop(first);
    let release = controller.commands.try_recv()?;
    controller.command(release);
    assert!(controller.scopes.is_empty());

    let next = coordinator.acquire(IncomingScope::Topics(vec![]));
    assert!(!controller.stop_if_idle());
    let acquire = controller.commands.try_recv()?;
    controller.command(acquire);
    assert_eq!(controller.scopes.len(), 1);
    drop(next);
    let release = controller.commands.try_recv()?;
    controller.command(release);
    drop(coordinator);
    assert!(controller.stop_if_idle());
    assert!(
        controller
            .context
            .incoming_runtime()
            .coordinator
            .lock()
            .is_none()
    );
    assert!(controller.commands.is_closed());
}
