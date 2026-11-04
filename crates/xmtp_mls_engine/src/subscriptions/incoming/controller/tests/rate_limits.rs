//! Incoming controller tests: rate limits.

use super::*;

fn rate_limited(category: &str, delay: Duration) -> NetworkError {
    NetworkError::new(xmtp_api::ApiError::Api(NetworkError::new(
        xmtp_api_grpc::error::GrpcError::Status(xmtp_api_grpc::error::rate_limited_status(
            category, delay,
        )),
    )))
}

/// A replacement controller cannot open inside the delay an earlier one was given.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_replacement_controller_waits_out_a_rate_limited_open() {
    let alix = EngineTestPeer::new().await;
    let topic = Topic::new_group_message(GroupId::generate());
    let mut original = controller(alix.context.clone());
    original.transport.factory = Some(Arc::new(PendingFactory));
    add_scope(&mut original, 1, &topic);
    original.start_open();
    assert_eq!(original.transport.generation, 1);
    original.opened(Err(rate_limited("stream_rent", Duration::from_secs(30))));
    drop(original);

    let mut recreated = controller(alix.context.clone());
    recreated.transport.factory = Some(Arc::new(PendingFactory));
    add_scope(&mut recreated, 1, &topic);
    recreated.transport.wake();
    recreated.start_open();
    assert_eq!(
        recreated.transport.generation, 0,
        "no open inside the delay"
    );
}

/// A rate-limited read leaves the live stream open, and only that topic's
/// read waits out the delay: another topic still reads.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_read_keeps_the_stream_and_waits_only_its_topic() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let refused = Topic::new_group_message(GroupId::generate());
    let other = Topic::new_group_message(GroupId::generate());
    let stream =
        coordinator.acquire_stream(IncomingScope::Topics(vec![refused.clone(), other.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    let scope = controller.scopes.get_mut(&stream.id).unwrap();
    for topic in [&refused, &other] {
        scope.topics.insert(topic.clone());
        scope.targets.insert(topic.clone(), Cursor(5));
    }
    controller.transport.requested.insert(refused.clone());
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    controller.read_queue.push_back(refused.clone());
    controller.read_queue.push_back(other.clone());
    controller.read_finished((
        refused.clone(),
        Some(Cursor(0)),
        ReadOutcome::Fetched(Err(xmtp_mls_core::mls_store::MlsStoreError::Api(
            rate_limited_api("request", Duration::from_secs(300)),
        ))),
    ));
    assert!(
        matches!(controller.transport.state, TransportState::Streaming(_)),
        "a refused read must not end the stream"
    );
    // Due again by the poll interval; only the delay holds it now.
    controller
        .topics
        .get_mut(&refused)
        .unwrap()
        .receipt
        .last_read = None;
    controller.start_read();
    assert!(controller.read.is_some(), "another topic still reads");
    assert!(
        controller.receipt(&refused).last_read.is_none(),
        "no read of the refused topic inside the delay"
    );
    assert!(controller.receipt(&other).last_read.is_some());
}

/// A topic refused with a rate limit reads again once its delay ends.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_topic_reads_again_after_its_delay() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let refused = Topic::new_group_message(GroupId::generate());
    let stream = coordinator.acquire_stream(IncomingScope::Topics(vec![refused.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    let scope = controller.scopes.get_mut(&stream.id).unwrap();
    scope.topics.insert(refused.clone());
    scope.targets.insert(refused.clone(), Cursor(5));
    controller.transport.requested.insert(refused.clone());
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    controller.read_queue.push_back(refused.clone());
    controller.read_finished((
        refused.clone(),
        Some(Cursor(0)),
        ReadOutcome::Fetched(Err(xmtp_mls_core::mls_store::MlsStoreError::Api(
            rate_limited_api("request", Duration::from_millis(50)),
        ))),
    ));
    xmtp_common::time::sleep(Duration::from_millis(60)).await;
    controller
        .topics
        .get_mut(&refused)
        .unwrap()
        .receipt
        .last_read = None;
    controller.start_read();
    assert!(
        controller.receipt(&refused).last_read.is_some(),
        "the topic reads again after the delay"
    );
}

/// A stream that ends with a rate limit, as the bidi transport reports it,
/// holds the next open.
// verifies: RATE-041
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_stream_end_holds_the_next_open() {
    use xmtp_api_backend::{ConnectionFailure, TransportError};
    let alix = EngineTestPeer::new().await;
    let topic = Topic::new_group_message(GroupId::generate());
    let mut controller = controller(alix.context.clone());
    controller.transport.factory = Some(Arc::new(PendingFactory));
    add_scope(&mut controller, 1, &topic);
    controller.start_open();
    assert_eq!(controller.transport.generation, 1);
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    let end = NetworkError::new(TransportError::Wire(std::sync::Arc::new(
        ConnectionFailure::Wire(rate_limited("stream_rent", Duration::from_secs(30))),
    )));
    controller.incoming(Some(Err(end)));
    controller.transport.wake();
    controller.start_open();
    assert_eq!(
        controller.transport.generation, 1,
        "no reopen inside the delay"
    );
}

fn rate_limited_api(category: &str, delay: Duration) -> xmtp_api::ApiError {
    xmtp_api::ApiError::Api(NetworkError::new(xmtp_api_grpc::error::GrpcError::Status(
        xmtp_api_grpc::error::rate_limited_status(category, delay),
    )))
}

/// The head that needed a refused identity proof retries after the delay,
/// and other dependency fetches still start: the hold is per request.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_identity_proof_floors_only_its_head() {
    use xmtp_db::incoming_envelope::{NetworkEntityKind, NewIncomingEnvelope};
    let alix = EngineTestPeer::new().await;
    let mut controller = controller(alix.context.clone());
    let topic = Topic::new_group_message(GroupId::generate());
    let key = topic_key(&topic)?;
    alix.context.db().admit_ordered_batch(
        &key,
        Cursor(0),
        &[NewIncomingEnvelope {
            sequence_id: Cursor(1),
            envelope: vec![1],
        }],
        alix.context
            .incoming_runtime()
            .policy()
            .incoming_limits(NetworkEntityKind::Group),
    )?;
    let requirement = IdentityRequirement {
        inbox_id: alix.inbox_id().to_string(),
        sequence_id: 1,
    };
    let refused = identity_key(&requirement);
    controller.dependency_registry.attach(
        DependencyParent::GroupHead(topic, Cursor(1)),
        refused.clone(),
    );
    let other = IdentityRequirement {
        inbox_id: "other".into(),
        sequence_id: 1,
    };
    controller.dependency_registry.attach(
        DependencyParent::IdentityHead(Topic::new_group_message(GroupId::generate()), Cursor(1)),
        identity_key(&other),
    );
    let DependencyKey::Identity(requirements) = refused else {
        unreachable!()
    };
    let delay = Duration::from_secs(300);
    let before = xmtp_common::time::now_ns();
    controller.dependency_finished(DependencyResult::Identity(
        requirements,
        Err(
            xmtp_mls_identity::identity_updates::IdentityDependencyError::IdentityUpdate(Box::new(
                xmtp_mls_identity::identity_updates::IdentityUpdateError::Api(rate_limited_api(
                    "request", delay,
                )),
            )),
        ),
    ));
    let head = alix.context.db().first_pending_envelope(&key)?.unwrap();
    assert!(head.retry_at_ns >= before + delay.as_nanos() as i64);
    controller.start_dependencies();
    assert_eq!(
        controller.dependencies.len(),
        1,
        "other fetches still start"
    );
}

/// A refused target read is not sent again before its delay, and the delay
/// holds only its own topics: once the fallback interval has passed, a
/// capture for another topic still starts.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_target_read_waits_its_delay() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let refused = Topic::new_group_message(GroupId::generate());
    let other = Topic::new_group_message(GroupId::generate());
    let stream =
        coordinator.acquire_stream(IncomingScope::Topics(vec![refused.clone(), other.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    let scope = controller.scopes.get_mut(&stream.id).unwrap();
    scope.topics.extend([refused.clone(), other.clone()]);
    let generation = scope.generation;
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    let delay = Duration::from_secs(300);
    let before = Instant::now();
    controller.targets_request = Some(std::collections::HashSet::from([refused.clone()]));
    controller.targets_finished((
        vec![(stream.id, generation)],
        Err(NetworkError::new(rate_limited_api("request", delay))),
    ));
    assert!(
        controller.targets_retry_at? < before + delay,
        "the delay holds only the refused topics"
    );
    // The fallback interval has passed; only the delay holds the refused topic.
    controller.targets_retry_at = None;
    controller
        .transport
        .registered
        .extend([refused.clone(), other.clone()]);
    controller.start_targets();
    assert_eq!(
        controller.targets_request,
        Some(std::collections::HashSet::from([other.clone()]))
    );
}

/// A refused prefix target read is not sent again before its delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_prefix_target_read_waits_its_delay() {
    use super::processing::PrefixTarget;
    let alix = EngineTestPeer::new().await;
    let mut controller = controller(alix.context.clone());
    controller.prefix_holds.insert(
        Cursor(1),
        PrefixHold {
            deadline_ns: 0,
            target: PrefixTarget::Requested,
        },
    );
    let delay = Duration::from_secs(300);
    let before = Instant::now();
    controller.dependency_finished(DependencyResult::PrefixTarget(
        Cursor(1),
        Topic::new_group_message(GroupId::generate()),
        Err(rate_limited_api("request", delay)),
    ));
    let PrefixTarget::Unrequested {
        retry_at: Some(retry_at),
    } = controller.prefix_holds[&Cursor(1)].target
    else {
        panic!("the read retries later");
    };
    assert!(retry_at >= before + delay);
}

/// A rate limit says nothing about a topic, so it keeps the topic's
/// permanent-error backoff.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
fn a_rate_limited_read_keeps_the_topic_failure_backoff() {
    let mut controller = controller(context());
    let topic = Topic::new_group_message(GroupId::generate());
    for _ in 0..3 {
        controller.receive_error(
            topic.clone(),
            IncomingError::Store(xmtp_mls_core::mls_store::MlsStoreError::Api(
                xmtp_api::ApiError::InvalidResponse("cursor order"),
            )),
        );
    }
    assert_eq!(controller.receipt(&topic).blocked_failures, 3);
    controller.receive_error(
        topic.clone(),
        IncomingError::Store(xmtp_mls_core::mls_store::MlsStoreError::Api(
            rate_limited_api("request", Duration::from_secs(1)),
        )),
    );
    assert_eq!(controller.receipt(&topic).blocked_failures, 3);
}
