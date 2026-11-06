//! Incoming controller tests: welcome prefixes.

use super::*;

/// Suspended receipt follows only parents within the fixed Welcome target.
#[xmtp_common::test(unwrap_try = true)]
async fn suspended_welcome_barrier_receives_only_its_required_group_prefixes() {
    let alix = EngineTestPeer::new().await;
    let mut controller = controller(alix.context.clone());
    controller.transport.factory = Some(Arc::new(SuspendedFactory));
    let group_id = GroupId::generate();
    let topic = Topic::new_group_message(group_id);
    let key = topic_key(&topic)?;
    let later = Topic::new_group_message(GroupId::generate());
    let later_key = topic_key(&later)?;
    let welcome = Topic::new_welcome_message(alix.context.installation_id());
    add_scope(&mut controller, 1, &topic);
    add_barrier_scope(
        &mut controller,
        2,
        &welcome,
        Cursor(40),
        Instant::now() + Duration::from_secs(1),
    );
    for (cursor, (group, anchor)) in [
        (Cursor(30), (group_id, Cursor(10))),
        (Cursor(35), (group_id, Cursor(15))),
        (Cursor(50), (group_id, Cursor(20))),
        (
            Cursor(60),
            (GroupId::try_from(later.identifier())?, Cursor(20)),
        ),
    ] {
        controller.dependency_registry.attach(
            DependencyParent::Welcome(cursor),
            DependencyKey::GroupPrefix(group, anchor),
        );
    }
    controller.reconcile()?;
    let now = Instant::now();
    assert!(controller.read_due(&topic, &key, now)?);
    assert!(!controller.read_due(&later, &later_key, now)?);

    controller.command(Command::Release(2));
    controller.reconcile()?;
    assert!(
        !controller.read_due(&topic, &key, now)?,
        "a cancelled parent cannot authorize suspended prefix receipt"
    );
    add_barrier_scope(
        &mut controller,
        3,
        &welcome,
        Cursor(40),
        now + Duration::from_secs(1),
    );
    controller.reconcile()?;
    for (after, received) in [(0, 10), (10, 15)] {
        controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(
            OrderedEnvelopeBatch {
                topic: topic.clone(),
                after: Cursor(after),
                envelopes: vec![ReceivedEnvelope::from(wire::ServerEnvelope {
                    meta: Some(meta(&topic, received)),
                    envelope: Some(wire::ClientEnvelope::default()),
                })],
            },
        ))));
        assert_eq!(controller.read_due(&topic, &key, now)?, received < 15);
    }
    assert_eq!(
        controller.context.db().topic_progress(&key)?.received,
        Cursor(15)
    );
    assert!(
        !controller.read_due(&later, &later_key, now)?,
        "later Welcome parents stay suspended after required receipt completes"
    );
}

/// A Welcome prefix uses its parent's fixed wait and deadline.
#[xmtp_common::test(unwrap_try = true)]
async fn welcome_prefix_fallback_keeps_the_parent_barrier_deadline() {
    let alix = EngineTestPeer::new().await;
    let mut controller = controller(alix.context.clone());
    controller.transport.factory = Some(Arc::new(SuspendedFactory));
    let group_id = GroupId::generate();
    let topic = Topic::new_group_message(group_id);
    let key = topic_key(&topic)?;
    let welcome = Topic::new_welcome_message(alix.context.installation_id());
    let settings = controller.context.incoming_runtime().policy().clone();
    let now = Instant::now();
    add_barrier_scope(
        &mut controller,
        1,
        &welcome,
        Cursor(40),
        now + settings.receiver_fallback_interval * 3,
    );
    controller.dependency_registry.attach(
        DependencyParent::Welcome(Cursor(30)),
        DependencyKey::GroupPrefix(group_id, Cursor(10)),
    );
    controller.reconcile()?;
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    controller.registered([(topic.clone(), Cursor(100)), (welcome.clone(), Cursor(90))].into());
    let started = controller.scopes[&1].receipt_wait_started;
    assert!(!controller.read_due(&topic, &key, started)?);
    assert!(controller.read_due(&topic, &key, started + settings.receiver_fallback_interval)?);

    add_barrier_scope(
        &mut controller,
        2,
        &welcome,
        Cursor(40),
        now + settings.receiver_fallback_interval / 2,
    );
    controller.reconcile()?;
    let urgent = controller.scopes[&2].receipt_wait_started;
    assert!(
        controller.read_due(&topic, &key, urgent)?,
        "a required prefix cannot wait beyond its Welcome barrier deadline"
    );
    controller
        .topics
        .entry(topic.clone())
        .or_default()
        .receipt
        .last_read = Some(urgent);
    assert!(!controller.read_due(
        &topic,
        &key,
        urgent + settings.active_database_poll_interval / 2,
    )?);
    assert!(controller.read_due(
        &topic,
        &key,
        urgent + settings.active_database_poll_interval,
    )?);
}

/// A rejoin must wait while the old group is active, then resume after removal
/// even when the retired group's processed prefix stops before the join anchor.
/// No newest-position read runs before the hold's deadline; the first pass
/// after it starts one.
// verifies: JOIN-042, JOIN-092
#[xmtp_common::test(unwrap_try = true)]
async fn removal_wakes_a_welcome_before_its_join_anchor() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let HeldReadd {
        alix_group,
        bo_group,
        welcome,
        removal,
    } = held_readd(&alix, &bo).await;
    let group_topic = Topic::new_group_message(bo_group.group_id);
    let mut controller = controller(bo.context.clone());
    let welcome_topic = Topic::new_welcome_message(bo.context.installation_id());
    let welcome_key = topic_key(&welcome_topic)?;
    controller.read_queue.push_back(welcome_topic);
    controller.welcome_blocked_scan = None;
    assert!(!controller.process_ready(&mut false).await);
    assert!(controller.dependencies.is_empty());
    assert!(bo_group.is_active()?);
    assert!(
        bo.context
            .db()
            .pending_envelope(&welcome_key, welcome.cursor)?
            .is_some()
    );
    assert_eq!(controller.dependency_registry.prefixes().count(), 1);
    let (cursor, group, anchor) = controller.dependency_registry.prefixes().next().unwrap();
    assert_eq!(cursor, welcome.cursor);
    assert_eq!(group, bo_group.group_id);
    assert!(removal.received < anchor);
    // The deadline passes while the controller runs: the next pass reads.
    controller
        .prefix_holds
        .get_mut(&welcome.cursor)
        .unwrap()
        .deadline_ns = 0;
    assert!(!controller.process_ready(&mut false).await);
    assert_eq!(controller.dependencies.len(), 1);

    controller.read_queue.push_front(group_topic);
    for _ in 0..removal.inserted {
        assert!(controller.process_ready(&mut false).await);
    }
    assert!(
        bo.context
            .db()
            .pending_envelope(&welcome_key, welcome.cursor)?
            .is_none()
    );
    assert_eq!(controller.dependency_registry.prefixes().count(), 0);
    assert!(bo_group.is_active()?);
    assert_eq!(
        bo_group.epoch_authenticator().await?,
        alix_group.epoch_authenticator().await?
    );
}

/// A held re-add (`held_readd`) whose stored deadline has passed, as for a
/// client restarted after it.
struct PrefixDeadlineRestart {
    /// The restarted controller after its first pass, with live work suspended
    /// when the caller asked for it.
    controller: Controller<EngineTestContext>,
    alix_group: MlsGroup<EngineTestContext>,
    bo_group: MlsGroup<EngineTestContext>,
    welcome: Cursor,
    anchor: Cursor,
    removal: xmtp_db::incoming_envelope::AdmissionResult,
}

async fn restart_after_prefix_deadline(
    alix: &EngineTestPeer,
    bo: &EngineTestPeer,
    suspended: bool,
) -> PrefixDeadlineRestart {
    use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
    use xmtp_db::{ConnectionExt, refresh_state::EntityKind, schema::incoming_envelopes};

    let HeldReadd {
        alix_group,
        bo_group,
        welcome,
        removal,
    } = held_readd(alix, bo).await;
    // The first classification stores the hold's deadline.
    WelcomeService::new(bo.context.clone())
        .process_pending_welcomes_once(&[])
        .unwrap();
    // The deadline passes while the client is stopped.
    bo.context
        .db()
        .raw_query(|conn| {
            diesel::update(incoming_envelopes::table.find((
                bo.context.installation_id().to_vec(),
                EntityKind::Welcome,
                welcome.cursor.0 as i64,
            )))
            .set((
                incoming_envelopes::retry_expires_at_ns.eq(Some(1i64)),
                incoming_envelopes::retry_at_ns.eq(0i64),
            ))
            .execute(conn)
        })
        .unwrap();

    // A restarted controller attempts the Welcome again on its first pass.
    let mut controller = controller(bo.context.clone());
    controller
        .read_queue
        .push_back(Topic::new_welcome_message(bo.context.installation_id()));
    controller.welcome_blocked_scan = None;
    if suspended {
        controller.transport.factory = Some(Arc::new(SuspendedFactory));
    }
    assert!(!controller.process_ready(&mut false).await);
    let (cursor, group, anchor) = controller.dependency_registry.prefixes().next().unwrap();
    assert_eq!((cursor, group), (welcome.cursor, bo_group.group_id));
    assert!(removal.received < anchor);
    PrefixDeadlineRestart {
        controller,
        alix_group,
        bo_group,
        welcome: welcome.cursor,
        anchor,
        removal,
    }
}

/// A coordinator restarted after a group-prefix deadline must read the group
/// topic's newest position before it decides, once live work resumes, and keep
/// receiving the topic meanwhile. A topic that reaches the anchor keeps the
/// Welcome until the removal is processed, and the re-add joins.
// verifies: JOIN-092
#[xmtp_common::test(unwrap_try = true)]
async fn expired_prefix_wait_catches_up_before_deciding() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let PrefixDeadlineRestart {
        mut controller,
        alix_group,
        bo_group,
        welcome,
        anchor,
        removal,
    } = restart_after_prefix_deadline(&alix, &bo, true).await;
    let welcome_key = topic_key(&Topic::new_welcome_message(bo.context.installation_id()))?;
    let group_topic = Topic::new_group_message(bo_group.group_id);
    let pending = || bo.context.db().pending_envelope(&welcome_key, welcome);
    assert!(pending()?.is_some());
    assert!(bo_group.is_active()?);
    assert!(controller.dependencies.is_empty(), "suspended: no read");
    // After resuming, one newest-position read runs; a later pass does not
    // repeat it, and the group topic keeps its network interest.
    controller.transport.factory = None;
    assert!(!controller.process_ready(&mut false).await);
    assert_eq!(controller.dependencies.len(), 1);
    assert!(!controller.process_ready(&mut false).await);
    assert_eq!(controller.dependencies.len(), 1);
    controller.reconcile()?;
    assert!(controller.interested().contains(&group_topic));

    let result =
        xmtp_common::time::timeout(Duration::from_secs(10), controller.dependencies.next())
            .await?
            .unwrap();
    let DependencyResult::PrefixTarget(cursor, _, Ok(target)) = &result else {
        panic!("expected the group prefix target");
    };
    assert_eq!(*cursor, welcome);
    assert!(*target >= anchor);
    controller.dependency_finished(result);
    // The Welcome waits while processing approaches the removal, with no
    // further read, and joins on the pass that processes it.
    for _ in 1..removal.inserted {
        assert!(controller.process_ready(&mut false).await);
        assert!(pending()?.is_some());
    }
    assert!(controller.process_ready(&mut false).await);
    assert!(controller.dependencies.is_empty());
    assert!(pending()?.is_none());
    assert!(bo_group.is_active()?);
    assert_eq!(
        bo_group.epoch_authenticator().await?,
        alix_group.epoch_authenticator().await?
    );
}

/// After the deadline, a newest read below the anchor that processing has
/// reached rejects the Welcome and leaves the held group unchanged. A failed
/// read, or a target processing has not reached, keeps the Welcome. The read
/// results are injected; the rejecting one models a lagging copy of the store.
// verifies: JOIN-092
#[xmtp_common::test(unwrap_try = true)]
async fn expired_prefix_wait_rejects_below_a_reached_final_target() {
    use super::processing::PrefixTarget;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let PrefixDeadlineRestart {
        mut controller,
        bo_group,
        welcome,
        anchor,
        removal,
        ..
    } = restart_after_prefix_deadline(&alix, &bo, false).await;
    let welcome_topic = Topic::new_welcome_message(bo.context.installation_id());
    let welcome_key = topic_key(&welcome_topic)?;
    let group_topic = Topic::new_group_message(bo_group.group_id);
    let held = || -> Result<bool, xmtp_db::StorageError> {
        Ok(bo
            .context
            .db()
            .pending_envelope(&welcome_key, welcome)?
            .is_some()
            && bo.context.db().read_last_rejection(&welcome_key)?.is_none())
    };
    let epoch = bo_group.epoch().await?;
    let processed = bo
        .context
        .db()
        .topic_progress(&StreamTopic::group(bo_group.group_id))?
        .processed;
    assert!(processed < removal.received && removal.received < anchor);

    // A failed read keeps the Welcome and is issued again once due.
    controller.dependencies.clear();
    controller.dependency_finished(DependencyResult::PrefixTarget(
        welcome,
        group_topic.clone(),
        Err(xmtp_api::ApiError::InvalidResponse("newest metadata")),
    ));
    assert!(!controller.process_ready(&mut false).await);
    assert!(held()?);
    assert!(controller.dependencies.is_empty());
    if let PrefixTarget::Unrequested { retry_at } =
        &mut controller.prefix_holds.get_mut(&welcome).unwrap().target
    {
        *retry_at = Some(Instant::now());
    }
    assert!(!controller.process_ready(&mut false).await);
    assert_eq!(controller.dependencies.len(), 1);

    // Processing has not reached this target, and the removal at or below it
    // could still let the re-add join: the watch stays.
    controller.dependency_finished(DependencyResult::PrefixTarget(
        welcome,
        group_topic.clone(),
        Ok(removal.received),
    ));
    assert!(!controller.process_ready(&mut false).await);
    assert!(held()?);
    assert_eq!(controller.dependency_registry.prefixes().count(), 1);

    controller.dependency_finished(DependencyResult::PrefixTarget(
        welcome,
        group_topic,
        Ok(processed),
    ));
    assert!(controller.process_ready(&mut false).await);
    assert!(
        bo.context
            .db()
            .pending_envelope(&welcome_key, welcome)?
            .is_none()
    );
    let rejection = bo.context.db().read_last_rejection(&welcome_key)?.unwrap();
    assert_eq!(rejection.code, "group_prefix_expired");
    assert_eq!(rejection.sequence_id, welcome);
    assert_eq!(controller.dependency_registry.prefixes().count(), 0);
    assert!(bo_group.is_active()?);
    assert_eq!(bo_group.epoch().await?, epoch);
    assert_eq!(
        bo.context
            .db()
            .topic_progress(&StreamTopic::group(bo_group.group_id))?
            .processed,
        processed
    );
}

/// Another process has already processed the removal, so this controller has
/// not retired the group. A newest read below the anchor that processing has
/// reached must not reject the re-add: the group is no longer active. The
/// controller releases the hold and joins on a later attempt.
// verifies: JOIN-092
#[xmtp_common::test(unwrap_try = true)]
async fn expired_prefix_wait_keeps_a_welcome_for_an_inactive_group() {
    use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
    use xmtp_db::{ConnectionExt, refresh_state::EntityKind, schema::incoming_envelopes};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let PrefixDeadlineRestart {
        mut controller,
        alix_group,
        bo_group,
        welcome,
        removal,
        ..
    } = restart_after_prefix_deadline(&alix, &bo, false).await;
    let welcome_key = topic_key(&Topic::new_welcome_message(bo.context.installation_id()))?;
    for _ in 0..removal.inserted {
        bo_group.process_pending_group_head(None)?;
    }
    assert!(!bo_group.is_active()?);
    let processed = bo
        .context
        .db()
        .topic_progress(&StreamTopic::group(bo_group.group_id))?
        .processed;

    controller.dependency_finished(DependencyResult::PrefixTarget(
        welcome,
        Topic::new_group_message(bo_group.group_id),
        Ok(processed),
    ));
    assert!(!controller.process_ready(&mut false).await);
    assert!(
        bo.context
            .db()
            .pending_envelope(&welcome_key, welcome)?
            .is_some()
    );
    assert!(bo.context.db().read_last_rejection(&welcome_key)?.is_none());

    // Once its retry time comes, the next pass attempts the Welcome again.
    bo.context.db().raw_query(|conn| {
        diesel::update(incoming_envelopes::table.find((
            bo.context.installation_id().to_vec(),
            EntityKind::Welcome,
            welcome.0 as i64,
        )))
        .set(incoming_envelopes::retry_at_ns.eq(0i64))
        .execute(conn)
    })?;
    assert!(controller.process_ready(&mut false).await);
    assert!(
        bo.context
            .db()
            .pending_envelope(&welcome_key, welcome)?
            .is_none()
    );
    assert!(bo.context.db().read_last_rejection(&welcome_key)?.is_none());
    assert_eq!(
        bo_group.epoch_authenticator().await?,
        alix_group.epoch_authenticator().await?
    );
}

/// A pointer whose fetch is still running must not occupy every ready batch;
/// an independent later Welcome must join without rewriting the held row.
// verifies: PROC-013
#[xmtp_common::test(unwrap_try = true)]
async fn an_in_flight_pointer_does_not_starve_a_later_welcome() {
    use crate::groups::welcome_sync::pending_welcome_for_test;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let _bo2 = bo.new_installation().await;
    let _bo3 = bo.new_installation().await;
    let first = alix.create_group(None, None)?;
    first.add_members(&[bo.inbox_id()]).await?;
    let later = alix.create_group(None, None)?;
    later.add_members(&[bo.inbox_id()]).await?;
    let mut welcomes = bo
        .context
        .api()
        .query_welcome_messages(bo.context.installation_id())
        .await?;
    welcomes.sort_by_key(|welcome| welcome.cursor);
    assert_eq!(welcomes.len(), 2);
    assert!(matches!(
        welcomes[0].variant,
        xmtp_proto::types::WelcomeMessageType::WelcomePointer(_)
    ));
    pending_welcome_for_test(&bo.context, &welcomes[1]).await?;
    let client = bo
        .with_stream_policy(|settings| settings.max_fetched_rows = 1)
        .await;
    let mut controller = controller(client.context.clone());
    let topic = Topic::new_welcome_message(client.context.installation_id());
    let key = topic_key(&topic)?;
    controller.read_queue.push_back(topic);
    controller.welcome_blocked_scan = None;
    controller.dependency_registry.attach(
        DependencyParent::Welcome(welcomes[0].cursor),
        DependencyKey::Welcome(welcomes[0].cursor),
    );
    // Keep the first fetch in flight without resolving it or advancing time.
    assert_eq!(controller.dependency_registry.start_queued(1).len(), 1);
    controller.process_ready(&mut false).await;
    assert!(
        controller
            .dependency_registry
            .contains(&DependencyParent::Welcome(welcomes[1].cursor))
    );
    let result =
        xmtp_common::time::timeout(Duration::from_secs(10), controller.dependencies.next())
            .await?
            .unwrap();
    controller.dependency_finished(result);
    assert!(client.context.db().find_group(&later.group_id)?.is_some());
    assert!(client.context.db().find_group(&first.group_id)?.is_none());
    let pending = client
        .context
        .db()
        .pending_envelope(&key, welcomes[0].cursor)?
        .unwrap();
    assert_eq!(pending.retry_at_ns, 0);
    assert!(!pending.blocked);
    assert!(pending.error_code.is_none());
    assert!(pending.retry_expires_at_ns.is_none());
    assert!(pending.unreadable_expires_at_ns.is_none());
}
