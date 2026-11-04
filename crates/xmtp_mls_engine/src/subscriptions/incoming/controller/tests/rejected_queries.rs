//! Incoming controller tests: rejected queries.

use super::*;

// verifies: API-284, PROC-038
#[xmtp_common::test(unwrap_try = true)]
async fn a_rejected_query_is_not_repeated_from_the_same_cursor() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let topic = Topic::new_welcome_message(alix.context.installation_id());
    let stream = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&stream.id)
        .unwrap()
        .topics
        .insert(topic.clone());
    controller.read_queue.push_back(topic.clone());
    let error =
        xmtp_mls_core::mls_store::MlsStoreError::Api(xmtp_api::ApiError::Api(NetworkError::new(
            xmtp_api_grpc::error::GrpcError::Status(tonic::Status::out_of_range("minimum page")),
        )));
    controller.read_finished((
        topic.clone(),
        Some(Cursor(0)),
        ReadOutcome::Fetched(Err(error)),
    ));
    controller.refresh_statuses();
    assert!(matches!(
        stream.recovery_snapshot().terminal,
        Some(crate::subscriptions::recovery::RecoveryFailure::Terminal(_))
    ));
    assert!(controller.receipt(&topic).blocked());
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
    controller
        .scopes
        .get_mut(&joining.id)
        .unwrap()
        .targets
        .insert(topic.clone(), Cursor(1));
    controller.start_read();
    let result = controller
        .read
        .take()
        .expect("fallback classification")
        .await;
    assert!(matches!(&result.2, ReadOutcome::Rejected(_)));
    controller.read_finished(result);
    assert!(matches!(
        joining.recovery_snapshot().terminal,
        Some(crate::subscriptions::recovery::RecoveryFailure::Terminal(_))
    ));
    // Even after the retry delay, the same durable cursor must not send the
    // rejected Query again. The actor can still serve a changed request.
    let receipt = &mut controller.topics.entry(topic.clone()).or_default().receipt;
    receipt.blocked_until = None;
    receipt.last_read = None;
    controller.start_read();
    let result = controller
        .read
        .take()
        .expect("retained classification")
        .await;
    assert!(matches!(&result.2, ReadOutcome::Rejected(_)));
    controller.read_finished(result);
    assert_eq!(controller.receipt(&topic).rejected_at, Some(Cursor(0)));

    let retained = controller.rejected_requests.lock()[0].1.clone();
    assert_eq!(
        controller.rejected_requests.lock()[0].0,
        RequestKey::Query(topic.clone(), Cursor(0))
    );
    drop(controller);
    drop(coordinator);
    drop(stream);

    // A fresh stream on the same client receives the retained cause. Its new
    // controller must not issue the same Query from durable cursor F.
    let (coordinator, mut recreated) = coordinated_controller(alix.context.clone());
    let fresh = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = recreated.commands.try_recv() {
        recreated.command(command);
    }
    recreated
        .scopes
        .get_mut(&fresh.id)
        .unwrap()
        .topics
        .insert(topic.clone());
    recreated
        .scopes
        .get_mut(&fresh.id)
        .unwrap()
        .targets
        .insert(topic.clone(), Cursor(1));
    recreated.read_queue.push_back(topic.clone());
    recreated.start_read();
    let result = recreated
        .read
        .take()
        .expect("retained classification")
        .await;
    assert!(matches!(&result.2, ReadOutcome::Rejected(_)));
    recreated.read_finished(result);
    assert_eq!(recreated.receipt(&topic).rejected_at, Some(Cursor(0)));
    assert_eq!(fresh.recovery_snapshot().failures, 0);
    let Some(crate::subscriptions::recovery::RecoveryFailure::Terminal(cause)) =
        fresh.recovery_snapshot().terminal
    else {
        panic!("the retained Query cause must terminate the fresh reader");
    };
    assert!(Arc::ptr_eq(&cause, &retained));
}

// verifies: API-284, PROC-039
#[xmtp_common::test(unwrap_try = true)]
async fn a_rejected_fallback_query_does_not_end_a_covered_fresh_stream() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let topic = Topic::new_group_message(GroupId::generate());
    let original = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&original.id)
        .unwrap()
        .topics
        .insert(topic.clone());
    controller.read_queue.push_back(topic.clone());
    let error =
        xmtp_mls_core::mls_store::MlsStoreError::Api(xmtp_api::ApiError::Api(NetworkError::new(
            xmtp_api_grpc::error::GrpcError::Status(tonic::Status::unimplemented("query")),
        )));
    controller.read_finished((
        topic.clone(),
        Some(Cursor(0)),
        ReadOutcome::Fetched(Err(error)),
    ));
    controller.transport.requested.insert(topic.clone());
    controller.transport.registered.insert(topic.clone());
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));

    let fresh = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    let scope = controller.scopes.get_mut(&fresh.id).unwrap();
    scope.topics.insert(topic.clone());
    scope.targets.insert(topic.clone(), Cursor(0));
    controller.start_read();
    assert!(controller.read.is_none());
    assert!(fresh.recovery_snapshot().terminal.is_none());
    assert_eq!(controller.receipt(&topic).rejected_at, Some(Cursor(0)));
}

// verifies: API-284
#[xmtp_common::test(unwrap_try = true)]
async fn a_changed_durable_cursor_clears_the_rejected_query_backoff() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let topic = Topic::new_group_message(GroupId::generate());
    let stream = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&stream.id)
        .unwrap()
        .topics
        .insert(topic.clone());
    controller.read_queue.push_back(topic.clone());
    let error =
        xmtp_mls_core::mls_store::MlsStoreError::Api(xmtp_api::ApiError::Api(NetworkError::new(
            xmtp_api_grpc::error::GrpcError::Status(tonic::Status::out_of_range("minimum page")),
        )));
    controller.read_finished((
        topic.clone(),
        Some(Cursor(0)),
        ReadOutcome::Fetched(Err(error)),
    ));
    assert!(controller.receipt(&topic).blocked());

    let limits = controller
        .context
        .incoming_runtime()
        .policy()
        .incoming_limits(NetworkEntityKind::Group);
    MlsStore::new(controller.context.clone()).admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: Cursor(0),
            envelopes: vec![ReceivedEnvelope::from(wire::ServerEnvelope {
                meta: Some(meta(&topic, 1)),
                envelope: Some(wire::ClientEnvelope::default()),
            })],
        },
        limits,
    )?;
    assert_eq!(
        controller
            .context
            .db()
            .topic_progress(&topic_key(&topic)?)?
            .received,
        Cursor(1)
    );
    controller
        .topics
        .entry(topic.clone())
        .or_default()
        .receipt
        .last_read = None;
    controller.start_read();
    let result = controller.read.take().expect("changed Query is due").await;
    assert_eq!(result.1, Some(Cursor(1)));
    assert!(matches!(&result.2, ReadOutcome::Fetched(_)));
    controller.read_finished(result);
    assert_eq!(controller.receipt(&topic).rejected_at, None);
    assert!(!controller.receipt(&topic).blocked());
}

// verifies: API-284, PROC-039
#[xmtp_common::test(unwrap_try = true)]
async fn a_covered_bidi_topic_does_not_poll_a_retained_query_cursor() {
    let alix = EngineTestPeer::new().await;
    let mut controller = controller(alix.context.clone());
    let topic = Topic::new_group_message(GroupId::generate());
    add_scope(&mut controller, 1, &topic);
    controller.read_queue.push_back(topic.clone());
    controller.rejected_requests.lock().push((
        RequestKey::Query(topic.clone(), Cursor(0)),
        Arc::new(IncomingError::UnsupportedTopic),
    ));
    let receipt = &mut controller.topics.entry(topic.clone()).or_default().receipt;
    receipt.rejected_at = Some(Cursor(0));
    receipt.blocked_until = Some(Instant::now() + Duration::from_secs(60));
    controller.transport.requested.insert(topic.clone());
    controller.transport.registered.insert(topic);
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    for _ in 0..100 {
        controller.start_read();
    }
    assert!(controller.read.is_none());
    assert_eq!(
        controller
            .receipt_progress_reads
            .load(std::sync::atomic::Ordering::Relaxed),
        0,
        "a covered topic must not read durable progress on each controller pass"
    );
    assert_eq!(
        controller.receipt(&controller.read_queue[0]).rejected_at,
        Some(Cursor(0))
    );
}

// verifies: API-284
#[xmtp_common::test(unwrap_try = true)]
async fn a_query_uses_the_cursor_checked_inside_its_future() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let topic = Topic::new_group_message(GroupId::generate());
    let stream = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    let scope = controller.scopes.get_mut(&stream.id).unwrap();
    scope.topics.insert(topic.clone());
    scope.targets.insert(topic.clone(), Cursor(2));
    controller.read_queue.push_back(topic.clone());
    controller.rejected_requests.lock().push((
        RequestKey::Query(topic.clone(), Cursor(0)),
        Arc::new(IncomingError::UnsupportedTopic),
    ));
    controller
        .topics
        .entry(topic.clone())
        .or_default()
        .receipt
        .rejected_at = Some(Cursor(0));
    controller.start_read();
    let future = controller.read.take().expect("the fallback Query is due");

    // Storage advances after the scheduler chooses Query and before the
    // future reads its cursor. The old rejection no longer matches the request.
    let limits = controller
        .context
        .incoming_runtime()
        .policy()
        .incoming_limits(NetworkEntityKind::Group);
    MlsStore::new(controller.context.clone()).admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: Cursor(0),
            envelopes: vec![ReceivedEnvelope::from(wire::ServerEnvelope {
                meta: Some(meta(&topic, 1)),
                envelope: Some(wire::ClientEnvelope::default()),
            })],
        },
        limits,
    )?;
    let (read_topic, cursor, result) = future.await;
    assert_eq!(read_topic, topic);
    assert_eq!(cursor, Some(Cursor(1)));
    assert!(matches!(result, ReadOutcome::Fetched(_)));
    assert!(stream.recovery_snapshot().terminal.is_none());
}

// verifies: API-284
#[xmtp_common::test(unwrap_try = true)]
async fn a_retained_rejection_does_not_poll_cursors_on_a_healthy_bidi_loop() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let topic = Topic::new_group_message(GroupId::generate());
    let other = Topic::new_group_message(GroupId::generate());
    controller.rejected_requests.lock().push((
        RequestKey::QueryNewest([other].into()),
        Arc::new(IncomingError::UnsupportedTopic),
    ));
    controller.transport.factory = Some(Arc::new(PendingFactory));
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    add_scope(&mut controller, 1, &topic);
    controller.transport.requested.insert(topic.clone());
    controller.transport.registered.insert(topic.clone());
    for _ in 0..100 {
        controller.start_open();
    }
    assert_eq!(controller.open_cursor_reads, 0);

    let fresh = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&fresh.id)
        .unwrap()
        .topics
        .insert(topic);
    controller.start_open();
    assert_eq!(controller.open_cursor_reads, 1);
    for _ in 0..100 {
        controller.start_open();
    }
    assert_eq!(controller.open_cursor_reads, 1);
}

// verifies: API-284
#[xmtp_common::test(unwrap_try = true)]
async fn an_internal_scope_does_not_repeat_a_rejected_newest_query() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    controller.transport.factory = None;
    let welcome = Topic::new_welcome_message(alix.context.installation_id());
    let worker = coordinator.acquire(IncomingScope::DeviceSyncGroups);
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&worker.id)
        .unwrap()
        .topics
        .insert(welcome.clone());
    controller.start_open();
    assert_eq!(controller.transport.generation, 1);

    let rejected = NetworkError::new(xmtp_api::ApiError::Api(NetworkError::new(
        xmtp_api_grpc::error::GrpcError::Status(tonic::Status::invalid_argument("topics")),
    )));
    controller.opened(Err(rejected));
    controller.transport.wake();
    controller.start_open();
    assert_eq!(controller.transport.generation, 1);

    // A new topic changes the request and lets the worker try again.
    let group = Topic::new_group_message(GroupId::generate());
    controller
        .scopes
        .get_mut(&worker.id)
        .unwrap()
        .topics
        .insert(group.clone());
    controller.start_open();
    assert_eq!(controller.transport.generation, 2);

    controller.opened(Err(NetworkError::new(xmtp_api::ApiError::Api(
        NetworkError::new(xmtp_api_grpc::error::GrpcError::Status(
            tonic::Status::unimplemented("topics"),
        )),
    ))));
    controller.transport.wake();
    controller.start_open();
    assert_eq!(controller.transport.generation, 2);

    // A new stream gets its own budget, but cannot resend the same wire payload.
    let fresh =
        coordinator.acquire_stream(IncomingScope::Topics(vec![welcome.clone(), group.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&fresh.id)
        .unwrap()
        .topics
        .extend([welcome, group]);
    controller.start_open();
    assert_eq!(controller.transport.generation, 2);
    assert!(fresh.recovery_snapshot().terminal.is_some());
    let changed = Topic::new_group_message(GroupId::generate());
    controller
        .scopes
        .get_mut(&worker.id)
        .unwrap()
        .topics
        .insert(changed);
    controller.start_open();
    assert_eq!(controller.transport.generation, 3);
}
