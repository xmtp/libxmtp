//! Incoming controller tests: rejected targets.

use super::*;

// verifies: API-284
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_wire_payloads_survive_controller_recreation_and_a_b_a_changes() {
    let alix = EngineTestPeer::new().await;
    let a = Topic::new_group_message(GroupId::generate());
    let b = Topic::new_group_message(GroupId::generate());
    let rejected = || {
        NetworkError::new(xmtp_api::ApiError::Api(NetworkError::new(
            xmtp_api_grpc::error::GrpcError::Status(tonic::Status::invalid_argument("payload")),
        )))
    };
    let mut original = controller(alix.context.clone());
    original.transport.factory = None;
    add_scope(&mut original, 1, &a);
    original.start_open();
    original.opened(Err(rejected()));
    assert_eq!(original.rejected_requests.lock().len(), 1);
    drop(original);

    let mut recreated = controller(alix.context.clone());
    recreated.transport.factory = None;
    add_scope(&mut recreated, 1, &a);
    recreated.start_open();
    assert_eq!(recreated.transport.generation, 0);
    recreated.scopes.remove(&1);
    add_scope(&mut recreated, 2, &b);
    recreated.start_open();
    assert_eq!(recreated.transport.generation, 1);
    recreated.opened(Err(rejected()));
    assert_eq!(recreated.rejected_requests.lock().len(), 2);

    recreated.scopes.remove(&2);
    add_scope(&mut recreated, 3, &a);
    recreated.transport.wake();
    recreated.start_open();
    assert_eq!(recreated.transport.generation, 1);
}

// verifies: API-284
#[xmtp_common::test(unwrap_try = true)]
async fn newest_rejection_blocks_target_capture_but_not_bidi_for_same_topics() {
    let alix = EngineTestPeer::new().await;
    let topic = Topic::new_group_message(GroupId::generate());
    let mut controller = controller(alix.context.clone());
    controller.transport.factory = None;
    add_scope(&mut controller, 1, &topic);
    controller.start_open();
    controller.opened(Err(NetworkError::new(xmtp_api::ApiError::Api(
        NetworkError::new(xmtp_api_grpc::error::GrpcError::Status(
            tonic::Status::out_of_range("newest"),
        )),
    ))));
    controller.transport.factory = Some(Arc::new(PendingFactory));
    controller.transport.wake();
    controller.start_open();
    assert_eq!(controller.transport.generation, 2);
    controller.registered([(topic.clone(), Cursor(10))].into());
    add_scope(&mut controller, 2, &topic);
    controller.start_targets();
    assert!(controller.targets.is_none());
    assert!(controller.scopes[&2].target_error.is_some());
}

// verifies: API-284
#[xmtp_common::test(unwrap_try = true)]
async fn a_rejected_target_query_does_not_block_a_valid_bidi_open() {
    let alix = EngineTestPeer::new().await;
    let mut controller = controller(alix.context.clone());
    controller.transport.factory = Some(Arc::new(PendingFactory));
    let topic = Topic::new_group_message(GroupId::generate());
    add_scope(&mut controller, 1, &topic);
    controller.start_open();
    assert_eq!(controller.transport.generation, 1);
    controller.registered([(topic.clone(), Cursor(10))].into());

    add_scope(&mut controller, 2, &topic);
    controller.start_targets();
    assert_eq!(
        controller.targets_request.as_ref().map(HashSet::len),
        Some(1)
    );
    controller.targets.take();
    let rejected = NetworkError::new(xmtp_api::ApiError::Api(NetworkError::new(
        xmtp_api_grpc::error::GrpcError::Status(tonic::Status::invalid_argument("target")),
    )));
    controller.targets_finished((vec![(2, 2)], Err(rejected)));
    let rejected = controller.rejected_requests.lock();
    assert_eq!(rejected.len(), 1);
    assert_eq!(
        rejected[0].0,
        RequestKey::QueryNewest([topic.clone()].into())
    );
    drop(rejected);

    controller.start_open();
    assert_eq!(controller.transport.generation, 1);
    assert!(controller.transport.registered.contains(&topic));
    controller.start_targets();
    assert!(
        controller.targets.is_none(),
        "the same target query stays rejected"
    );

    let other = Topic::new_group_message(GroupId::generate());
    add_scope(&mut controller, 3, &other);
    controller.transport.registered.insert(other);
    controller.start_targets();
    assert!(
        controller.targets.is_some(),
        "changed target set can be tried"
    );
}

// verifies: API-284, PROC-038
#[xmtp_common::test(unwrap_try = true)]
async fn a_rejected_new_scope_target_does_not_end_an_active_sibling() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let topic = Topic::new_group_message(GroupId::generate());
    let active = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&active.id)
        .unwrap()
        .topics
        .insert(topic.clone());
    controller.transport.requested.insert(topic.clone());
    controller.transport.state = TransportState::Unary;
    controller.registered([(topic.clone(), Cursor(10))].into());

    let joining = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&joining.id)
        .unwrap()
        .topics
        .insert(topic.clone());
    controller.start_targets();
    assert!(controller.targets.is_some());
    controller.targets.take();
    let rejected = NetworkError::new(xmtp_api::ApiError::Api(NetworkError::new(
        xmtp_api_grpc::error::GrpcError::Status(tonic::Status::unimplemented("target")),
    )));
    controller.targets_finished((vec![(joining.id, joining.id)], Err(rejected)));
    controller.refresh_statuses();
    assert_eq!(
        controller.transport.connection(),
        IncomingConnection::Connected
    );
    assert!(controller.transport.registered.contains(&topic));
    assert!(
        controller.state.consumer_recovery.lock()[&active.id]
            .snapshot
            .terminal
            .is_none()
    );
    assert!(
        controller.state.consumer_recovery.lock()[&joining.id]
            .snapshot
            .terminal
            .is_some()
    );
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(
        OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: Cursor(0),
            envelopes: vec![ReceivedEnvelope::from(wire::ServerEnvelope {
                meta: Some(meta(&topic, 1)),
                envelope: Some(wire::ClientEnvelope::default()),
            })],
        },
    ))));
    assert_eq!(
        controller
            .context
            .db()
            .topic_progress(&topic_key(&topic)?)?
            .received,
        Cursor(1),
        "the active sibling still receives from the shared source"
    );

    controller.retire_exhausted_streams();
    let replacement = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&replacement.id)
        .unwrap()
        .topics
        .insert(topic);
    controller.start_targets();
    assert!(controller.targets.is_none());
    assert!(
        controller.state.consumer_recovery.lock()[&replacement.id]
            .snapshot
            .terminal
            .is_some(),
        "the unchanged wire request returns its retained cause"
    );
}

// verifies: API-284, PROC-038
#[xmtp_common::test(unwrap_try = true)]
async fn a_caller_joining_during_a_rejected_target_query_gets_the_retained_error() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let topic = Topic::new_group_message(GroupId::generate());
    let active = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&active.id)
        .unwrap()
        .topics
        .insert(topic.clone());
    controller.transport.requested.insert(topic.clone());
    controller.transport.state = TransportState::Unary;
    controller.registered([(topic.clone(), Cursor(10))].into());

    let rejected_caller = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&rejected_caller.id)
        .unwrap()
        .topics
        .insert(topic.clone());
    controller.start_targets();
    controller.targets.take();

    let new_caller = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&new_caller.id)
        .unwrap()
        .topics
        .insert(topic.clone());
    let rejected = NetworkError::new(xmtp_api::ApiError::Api(NetworkError::new(
        xmtp_api_grpc::error::GrpcError::Status(tonic::Status::invalid_argument("old caller")),
    )));
    controller.targets_finished((
        vec![(rejected_caller.id, rejected_caller.id)],
        Err(rejected),
    ));
    controller.start_targets();
    assert!(controller.targets.is_none());
    controller.refresh_statuses();
    assert!(
        controller.state.consumer_recovery.lock()[&rejected_caller.id]
            .snapshot
            .terminal
            .is_some()
    );
    assert!(
        controller.state.consumer_recovery.lock()[&new_caller.id]
            .snapshot
            .terminal
            .is_some()
    );
    let retained = controller.rejected_requests.lock()[0].1.clone();
    let recovery = controller.state.consumer_recovery.lock();
    let Some(crate::subscriptions::recovery::RecoveryFailure::Terminal(cause)) =
        &recovery[&new_caller.id].snapshot.terminal
    else {
        panic!("the new caller must receive the retained terminal cause")
    };
    assert!(Arc::ptr_eq(cause, &retained));
    assert_eq!(recovery[&new_caller.id].query_failures, 0);
    drop(recovery);
    assert!(
        controller.state.consumer_recovery.lock()[&new_caller.id]
            .snapshot
            .healthy_since
            .is_none(),
        "registration without a fixed target does not reset recovery"
    );
    controller.retire_exhausted_streams();
    controller.start_targets();
    assert!(
        controller.targets.is_none(),
        "the same rejected wire request is not repeated"
    );
}

// verifies: API-284
#[xmtp_common::test(unwrap_try = true)]
async fn a_rejected_wire_open_stays_blocked_after_partial_or_full_registration() {
    let alix = EngineTestPeer::new().await;
    let topic = Topic::new_group_message(GroupId::generate());
    let second = Topic::new_group_message(GroupId::generate());
    let rejected = || {
        NetworkError::new(xmtp_api::ApiError::Api(NetworkError::new(
            xmtp_api_grpc::error::GrpcError::Status(tonic::Status::invalid_argument("wire")),
        )))
    };
    let mut initial = controller(alix.context.clone());
    initial.transport.factory = Some(Arc::new(PendingFactory));
    add_scope(&mut initial, 1, &topic);
    add_scope(&mut initial, 2, &second);
    initial.start_open();
    initial.registered([(topic.clone(), Cursor(10))].into());
    initial.incoming(Some(Err(rejected())));
    initial.transport.wake();
    initial.start_open();
    assert_eq!(initial.transport.generation, 1);

    let mut registered = controller(alix.context.clone());
    registered.transport.factory = Some(Arc::new(PendingFactory));
    add_scope(&mut registered, 1, &topic);
    registered.start_open();
    registered.registered([(topic, Cursor(10))].into());
    registered.incoming(Some(Err(rejected())));
    registered.transport.wake();
    registered.start_open();
    assert_eq!(registered.transport.generation, 1);
}

// verifies: API-284
#[xmtp_common::test(unwrap_try = true)]
async fn a_shared_wire_rejection_holds_each_lease_until_a_new_caller() {
    let alix = EngineTestPeer::new().await;
    let (first_coordinator, mut first) = coordinated_controller(alix.context.clone());
    let (second_coordinator, mut second) = coordinated_controller(alix.context.clone());
    let first_topic = Topic::new_group_message(GroupId::generate());
    let second_topic = Topic::new_group_message(GroupId::generate());
    for (coordinator, controller, topic) in [
        (&first_coordinator, &mut first, &first_topic),
        (&second_coordinator, &mut second, &second_topic),
    ] {
        controller.transport.factory = Some(Arc::new(PendingFactory));
        let caller = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
        while let Ok(command) = controller.commands.try_recv() {
            controller.command(command);
        }
        controller
            .scopes
            .get_mut(&caller.id)
            .unwrap()
            .topics
            .insert(topic.clone());
        controller.start_open();
        let rejected = NetworkError::new(xmtp_api::ApiError::Api(NetworkError::new(
            xmtp_api_grpc::error::GrpcError::Status(tonic::Status::invalid_argument("shared wire")),
        )));
        controller.incoming(Some(Err(rejected)));
        controller.refresh_statuses();
        assert!(
            controller.state.consumer_recovery.lock()[&caller.id]
                .snapshot
                .terminal
                .is_some(),
            "each affected app lease reports its terminal error"
        );
        controller.transport.wake();
        controller.start_open();
        assert_eq!(controller.transport.generation, 1);
    }

    let fresh = first_coordinator.acquire_stream(IncomingScope::Topics(vec![first_topic.clone()]));
    while let Ok(command) = first.commands.try_recv() {
        first.command(command);
    }
    first
        .scopes
        .get_mut(&fresh.id)
        .unwrap()
        .topics
        .insert(first_topic.clone());
    first.start_open();
    assert!(fresh.recovery_snapshot().terminal.is_some());
    assert_eq!(first.transport.generation, 1);
    first
        .scopes
        .get_mut(&fresh.id)
        .unwrap()
        .topics
        .insert(Topic::new_group_message(GroupId::generate()));
    first.start_open();
    assert_eq!(first.transport.generation, 2);
    second.start_open();
    assert_eq!(second.transport.generation, 1);
}

// verifies: PROC-038
#[xmtp_common::test(unwrap_try = true)]
async fn a_blocked_selected_topic_cannot_reset_health_through_a_sibling() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let welcome = Topic::new_welcome_message(alix.context.installation_id());
    let group = Topic::new_group_message(GroupId::generate());
    let stream =
        coordinator.acquire_stream(IncomingScope::Topics(vec![welcome.clone(), group.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&stream.id)
        .unwrap()
        .topics
        .extend([welcome.clone(), group.clone()]);
    controller.transport.state = TransportState::Unary;
    controller.transport.registered.insert(welcome.clone());
    controller
        .topics
        .entry(group.clone())
        .or_default()
        .receipt
        .blocked_until = Some(Instant::now() + Duration::from_secs(120));
    let now = Instant::now();
    controller.refresh_statuses_at(now);
    controller.refresh_statuses_at(now + crate::subscriptions::recovery::HEALTHY_PERIOD);
    assert!(stream.recovery_snapshot().healthy_since.is_none());
    assert!(stream.recovery_snapshot().outage_since.is_some());

    controller
        .topics
        .get_mut(&group)
        .unwrap()
        .receipt
        .blocked_until = None;
    controller.transport.registered.insert(group.clone());
    controller
        .scopes
        .get_mut(&stream.id)
        .unwrap()
        .targets
        .extend([(welcome, Cursor(0)), (group.clone(), Cursor(0))]);
    controller.refresh_statuses_at(now + Duration::from_secs(31));
    assert!(stream.recovery_snapshot().healthy_since.is_some());
    controller.refresh_statuses_at(now + Duration::from_secs(61));
    assert!(stream.recovery_snapshot().outage_since.is_none());
}
