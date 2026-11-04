//! Incoming controller tests: permanent failures.

use super::*;

#[xmtp_common::test(unwrap_try = true)]
fn permanent_source_and_topic_errors_stop_automatic_receipt() {
    let mut controller = controller(context());
    let topic = Topic::new_group_message(GroupId::generate());
    controller.read_queue.push_back(topic.clone());
    controller.receive_error(
        topic.clone(),
        IncomingError::Store(xmtp_mls_core::mls_store::MlsStoreError::Api(
            xmtp_api::ApiError::Envelope(xmtp_api_backend::envelope::EnvelopeError::Capacity),
        )),
    );
    controller.start_read();
    assert!(controller.read.is_none());
    assert!(controller.receipt(&topic).blocked());

    controller.clear_receipt_failure(&topic);
    // A permanent source error is reported to the host, but it must not stop
    // the unary read path: bounded Query is how the client recovers. The read
    // is now gated only by per-topic receipt state, never by stream health.
    controller.source_error(NetworkError::new(xmtp_api::ApiError::InvalidResponse(
        "cursor order",
    )));
    assert_eq!(
        controller.transport.connection(),
        IncomingConnection::Failed
    );
    assert_eq!(controller.transport.permanent_failures, 1);
    {
        let receipt = &mut controller.topics.entry(topic.clone()).or_default().receipt;
        receipt.blocked_failures = 1;
        receipt.blocked_until = Some(Instant::now() + Duration::from_secs(60));
    }
    controller.start_read();
    assert!(controller.read.is_none());
}

/// A scope acquired after a permanent stream failure must still receive work.
/// `source_error` clears the registration set, so a target request cannot be
/// what starts recovery: unary Query has to run without one.
#[xmtp_common::test(unwrap_try = true)]
fn a_scope_acquired_after_a_permanent_failure_still_reads() {
    let mut controller = controller(context());
    let topic = Topic::new_group_message(GroupId::generate());
    controller.source_error(NetworkError::new(xmtp_api::ApiError::InvalidResponse(
        "cursor order",
    )));
    assert_eq!(controller.transport.permanent_failures, 1);
    assert!(
        controller.transport.registered.is_empty(),
        "a failed source drops its registrations"
    );

    // The scope arrives after the failure, so it never had a registration and
    // never gets a fixed target. Neither may gate the bounded read.
    add_scope(&mut controller, 1, &topic);
    controller.read_queue.push_back(topic.clone());
    assert!(!controller.scopes[&1].targets.contains_key(&topic));

    // read_due is the gate the recovery depends on. A missing target skips the
    // scope loop but must not return early: an uncovered topic still falls
    // through to the bounded Query interval.
    let key = topic_key(&topic)?;
    assert!(
        controller.read_due(&topic, &key, Instant::now())?,
        "an unregistered topic with no target must fall back to Query"
    );
}

/// A capacity pause supersedes a permanent-error backoff. They are different
/// conditions, and leaving both set gates the topic after storage drains.
#[xmtp_common::test(unwrap_try = true)]
fn a_capacity_pause_clears_a_permanent_backoff() {
    let mut controller = controller(context());
    let topic = Topic::new_group_message(GroupId::generate());

    controller.receive_error(
        topic.clone(),
        IncomingError::Store(xmtp_mls_core::mls_store::MlsStoreError::Api(
            xmtp_api::ApiError::InvalidResponse("cursor order"),
        )),
    );
    assert!(controller.receipt(&topic).blocked());

    controller.receive_error(
        topic.clone(),
        IncomingError::Store(xmtp_mls_core::mls_store::MlsStoreError::Storage(
            xmtp_db::StorageError::Stream(xmtp_db::stream_storage::StreamStorageError::Capacity {
                scope: xmtp_db::stream_storage::BudgetScope::Kind,
            }),
        )),
    );
    assert!(controller.receipt(&topic).paused);
    assert!(!controller.receipt(&topic).blocked());
    assert!(!controller.receipt(&topic).failing());
}

/// An unrelated shorter disconnect must not erase a permanent-failure backoff.
/// Otherwise a per-topic read failure resets the receiver to reopening against
/// a broken backend roughly once a second.
#[xmtp_common::test(unwrap_try = true)]
fn a_shorter_disconnect_cannot_shorten_a_permanent_backoff() {
    let mut controller = controller(context());
    let permanent = || NetworkError::new(xmtp_api::ApiError::InvalidResponse("cursor order"));

    for _ in 0..5 {
        controller.source_error(permanent());
    }
    let scheduled = controller.transport.retry_at()?;

    // The per-topic path disconnects with the short fallback interval.
    controller.transport.disconnect(Duration::from_millis(1));
    assert_eq!(
        controller.transport.retry_at(),
        Some(scheduled),
        "a shorter delay must not shorten a longer pending retry"
    );
    assert!(controller.transport.backing_off());

    // A longer delay still applies.
    controller.transport.disconnect(Duration::from_secs(3600));
    assert!(controller.transport.retry_at()? > scheduled);
}

/// A permanent receipt error on one topic must not stop that topic forever.
/// The retry is delayed, then due, and a later success clears the streak.
#[xmtp_common::test(unwrap_try = true)]
fn a_permanent_receipt_error_retries_that_topic_with_backoff() {
    let mut controller = controller(context());
    let topic = Topic::new_group_message(GroupId::generate());
    let other = Topic::new_group_message(GroupId::generate());
    let permanent = || {
        IncomingError::Store(xmtp_mls_core::mls_store::MlsStoreError::Api(
            xmtp_api::ApiError::InvalidResponse("cursor order"),
        ))
    };
    assert!(!permanent().is_retryable());

    controller.receive_error(topic.clone(), permanent());
    assert!(controller.receipt(&topic).blocked());
    assert_eq!(controller.receipt(&topic).blocked_failures, 1);
    let first = controller.receipt(&topic).blocked_until;

    // An unrelated topic is untouched by another topic's failure.
    assert!(!controller.receipt(&other).blocked());
    assert_eq!(controller.receipt(&other).blocked_failures, 0);

    controller.receive_error(topic.clone(), permanent());
    assert_eq!(controller.receipt(&topic).blocked_failures, 2);
    assert!(
        controller.receipt(&topic).blocked_until > first,
        "the delay must grow with consecutive permanent failures"
    );

    // Once the delay elapses the topic is eligible again, and a success
    // clears the streak entirely.
    controller
        .topics
        .entry(topic.clone())
        .or_default()
        .receipt
        .blocked_until = Some(Instant::now());
    assert!(!controller.receipt(&topic).blocked());
    assert!(controller.receipt(&topic).failing());
    controller.clear_receipt_failure(&topic);
    assert!(!controller.receipt(&topic).failing());
}

// verifies: PROC-038, PROC-039
#[xmtp_common::test(unwrap_try = true)]
async fn failed_queries_spend_only_the_interested_stream_budget() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let failing = Topic::new_welcome_message(alix.context.installation_id());
    let healthy = Topic::new_group_message(GroupId::generate());
    let affected = coordinator.acquire_stream(IncomingScope::Topics(vec![failing.clone()]));
    let sibling = coordinator.acquire_stream(IncomingScope::Topics(vec![healthy]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller
        .scopes
        .get_mut(&affected.id)
        .unwrap()
        .topics
        .insert(failing.clone());
    for _ in 0..10 {
        let error = xmtp_mls_core::mls_store::MlsStoreError::Api(xmtp_api::ApiError::Api(
            NetworkError::new(xmtp_api_grpc::error::GrpcError::Status(
                tonic::Status::unavailable("query offline"),
            )),
        ));
        controller.read_finished((
            failing.clone(),
            Some(Cursor(0)),
            ReadOutcome::Fetched(Err(error)),
        ));
        controller.refresh_statuses();
    }
    assert_eq!(controller.transport.recovery.failures, 0);
    assert!(matches!(
        affected.recovery_snapshot().terminal,
        Some(crate::subscriptions::recovery::RecoveryFailure::Exhausted { attempts: 10, .. })
    ));
    assert!(sibling.recovery_snapshot().terminal.is_none());
}

/// A permanently failing source retries on a growing delay and recovers
/// without recreating the client.
#[xmtp_common::test(unwrap_try = true)]
fn a_permanent_source_error_retries_with_backoff_and_recovers() {
    let mut controller = controller(context());
    let permanent = || NetworkError::new(xmtp_api::ApiError::InvalidResponse("cursor order"));
    assert!(!permanent().is_retryable());

    controller.source_error(permanent());
    assert_eq!(controller.transport.permanent_failures, 1);
    assert!(controller.transport.backing_off());
    let first = controller.transport.retry_at();

    controller.source_error(permanent());
    assert_eq!(controller.transport.permanent_failures, 2);
    assert!(
        controller.transport.retry_at() > first,
        "the delay must grow with consecutive permanent failures"
    );

    // A wake advances a scheduled retry; no failure is terminal.
    controller.transport.wake();
    assert!(!controller.transport.backing_off());

    controller
        .transport
        .request(HashSet::from([Topic::new_group_message(
            GroupId::generate(),
        )]));
    assert!(controller.transport.can_open());

    controller.opened(Ok(Opened::Unary(TopicCursor::new())));
    assert_eq!(controller.transport.permanent_failures, 0);
    assert_eq!(
        controller.transport.connection(),
        IncomingConnection::Connected
    );
}

// verifies: AUTH-025, PROC-038
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
fn auth_lockout_counts_the_triggering_attempt_but_not_cooldown_refusals() {
    use xmtp_proto::api::{ApiClientError, AuthError};

    let mut controller = controller(context());
    let now = Instant::now();
    let mut budget =
        crate::subscriptions::recovery::RecoveryBudget::new(&controller.transport.recovery, now);
    let lockout_error = |auth| {
        NetworkError::new(xmtp_api_backend::TransportError::Open(
            xmtp_api_backend::OpenError::new(ApiClientError::Auth(auth)),
        ))
    };
    let backoff = RetryBackoff {
        initial: Duration::from_millis(1),
        max: Duration::from_secs(1),
    };
    controller.transport.fail(
        lockout_error(AuthError::ExhaustedAfterAttempt),
        Duration::from_millis(1),
        backoff,
    );
    assert_eq!(controller.transport.recovery.failures, 1);
    for _ in 0..12 {
        controller.transport.fail(
            lockout_error(AuthError::Exhausted),
            Duration::from_millis(1),
            backoff,
        );
        assert_eq!(controller.transport.recovery.failures, 1);
    }
    budget.check(
        &controller.transport.recovery,
        now + Duration::from_secs(599),
    )?;
    let failure = budget
        .check(
            &controller.transport.recovery,
            now + Duration::from_secs(600),
        )
        .unwrap_err();
    assert!(matches!(
        failure,
        crate::subscriptions::recovery::RecoveryFailure::Exhausted { attempts: 1, .. }
    ));
}

#[xmtp_common::test(unwrap_try = true)]
fn recovery_health_requires_registration_and_eof_keeps_a_cause() {
    let mut controller = controller(context());
    let topic = Topic::new_group_message(GroupId::generate());
    controller.transport.request([topic.clone()].into());
    controller.opened(Ok(Opened::Stream(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ))));
    assert!(controller.transport.recovery.healthy_since.is_none());
    controller.registered([(topic, Cursor(0))].into());
    assert!(controller.transport.recovery.healthy_since.is_some());
    controller.incoming(None);
    assert!(controller.transport.recovery.healthy_since.is_none());
    assert_eq!(controller.transport.recovery.failures, 1);
    assert!(matches!(
        controller.transport.recovery.error.as_deref(),
        Some(IncomingError::ReceiverEnded)
    ));
}

// verifies: PROC-038, PROC-039
#[xmtp_common::test(unwrap_try = true)]
async fn recovery_health_is_scoped_and_new_selection_needs_registration() {
    let alix = EngineTestPeer::new().await;
    let mut controller = controller(alix.context.clone());
    let first = Topic::new_group_message(GroupId::generate());
    let sibling = Topic::new_group_message(GroupId::generate());
    let replacement = Topic::new_group_message(GroupId::generate());
    add_scope(&mut controller, 1, &first);
    add_scope(&mut controller, 2, &sibling);
    controller
        .transport
        .request([first.clone(), sibling].into());
    controller.opened(Ok(Opened::Stream(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ))));
    controller.registered([(first, Cursor(0))].into());
    controller.refresh_statuses();
    assert!(
        controller.state.consumer_recovery.lock()[&1]
            .snapshot
            .healthy_since
            .is_some()
    );
    assert!(
        controller.state.consumer_recovery.lock()[&2]
            .snapshot
            .healthy_since
            .is_none()
    );
    controller.scopes.get_mut(&1).unwrap().topics = [replacement].into();
    controller.refresh_statuses();
    assert!(
        controller.state.consumer_recovery.lock()[&1]
            .snapshot
            .healthy_since
            .is_none()
    );
}

// verifies: PROC-038, PROC-039
#[xmtp_common::test(unwrap_try = true)]
async fn a_stream_opened_before_status_refresh_does_not_pay_prior_failures() {
    let alix = EngineTestPeer::new().await;
    let topic = Topic::new_welcome_message(alix.context.installation_id());
    for eof in [false, true] {
        let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
        let old = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
        while let Ok(command) = controller.commands.try_recv() {
            controller.command(command);
        }
        controller.reconcile()?;
        let fail = |controller: &mut Controller<_>| {
            if eof {
                controller.incoming(None);
            } else {
                controller.source_error(NetworkError::new(xmtp_api::ApiError::InvalidResponse(
                    "cursor order",
                )));
            }
        };
        for _ in 0..10 {
            fail(&mut controller);
        }
        // Acquire in the exact gap between the private failure event and the
        // next status refresh. It must see the completed failure increments.
        let replacement = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
        assert_eq!(replacement.recovery_snapshot().failures, 10);
        while let Ok(command) = controller.commands.try_recv() {
            controller.command(command);
        }
        controller.reconcile()?;
        controller.refresh_statuses();
        assert!(old.recovery_snapshot().terminal.is_some());
        assert!(replacement.recovery_snapshot().terminal.is_none());
        for _ in 0..9 {
            fail(&mut controller);
            controller.refresh_statuses();
            assert!(replacement.recovery_snapshot().terminal.is_none());
        }
        fail(&mut controller);
        controller.refresh_statuses();
        assert!(matches!(
            replacement.recovery_snapshot().terminal,
            Some(crate::subscriptions::recovery::RecoveryFailure::Exhausted { attempts: 10, .. })
        ));
    }
}

// verifies: PROC-021, PROC-038, EVENT-001, EVENT-027
#[xmtp_common::test(unwrap_try = true)]
async fn an_application_stream_retries_a_nonretryable_source_response_and_recovers() {
    let alix = EngineTestPeer::new().await;
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ConnectionStateChanged]),
        Some(8),
    );
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let topic = Topic::new_welcome_message(alix.context.installation_id());
    let application = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller.reconcile()?;
    controller.source_error(NetworkError::new(xmtp_api::ApiError::InvalidResponse(
        "cursor order",
    )));
    controller.refresh_statuses();
    assert!(controller.transport.backing_off());
    assert_eq!(
        application.snapshot().connection,
        IncomingConnection::Failed
    );
    assert!(application.recovery_snapshot().terminal.is_none());
    controller.transport.wake();
    controller.opened(Ok(Opened::Unary([(topic, Cursor(0))].into())));
    controller.refresh_statuses();
    assert_eq!(
        application.snapshot().connection,
        IncomingConnection::Connected
    );
    assert!(application.recovery_snapshot().healthy_since.is_some());
    assert!(application.recovery_snapshot().terminal.is_none());
    controller.refresh_statuses_at(Instant::now() + crate::subscriptions::recovery::HEALTHY_PERIOD);
    application.check_recovery()?;
    assert!(application.recovery_snapshot().terminal.is_none());
    application.close();
    let transitions: Vec<_> = events
        .drain()
        .into_iter()
        .filter_map(|item| match item.client {
            Some(xmtp_events::ClientEvent::ConnectionStateChanged(change)) => {
                Some((change.previous, change.current))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        transitions,
        [
            (
                xmtp_events::ConnectionState::Closed,
                xmtp_events::ConnectionState::Connecting
            ),
            (
                xmtp_events::ConnectionState::Connecting,
                xmtp_events::ConnectionState::Failed
            ),
            (
                xmtp_events::ConnectionState::Failed,
                xmtp_events::ConnectionState::Connected
            ),
            (
                xmtp_events::ConnectionState::Connected,
                xmtp_events::ConnectionState::Closed
            ),
        ]
    );
}

// verifies: AUTH-025, PROC-039
#[xmtp_common::test(unwrap_try = true)]
async fn a_terminal_credential_failure_stays_latched_only_for_its_stream() {
    let alix = EngineTestPeer::new().await;
    let (coordinator, mut controller) = coordinated_controller(alix.context.clone());
    let topic = Topic::new_welcome_message(alix.context.installation_id());
    let old = coordinator.acquire_stream(IncomingScope::Topics(vec![topic.clone()]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller.reconcile()?;
    controller.source_error(NetworkError::new(xmtp_proto::api::ApiClientError::Auth(
        xmtp_proto::api::AuthError::MissingCredential,
    )));
    controller.refresh_statuses();
    assert!(matches!(
        old.recovery_snapshot().terminal,
        Some(crate::subscriptions::recovery::RecoveryFailure::Terminal(_))
    ));
    controller.opened(Ok(Opened::Unary([(topic.clone(), Cursor(0))].into())));
    controller.refresh_statuses();
    controller.refresh_statuses_at(Instant::now() + crate::subscriptions::recovery::HEALTHY_PERIOD);
    assert!(matches!(
        old.recovery_snapshot().terminal,
        Some(crate::subscriptions::recovery::RecoveryFailure::Terminal(_))
    ));
    let replacement = coordinator.acquire_stream(IncomingScope::Topics(vec![topic]));
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller.reconcile()?;
    controller.refresh_statuses();
    assert!(replacement.recovery_snapshot().terminal.is_none());
}

// verifies: PROC-038, PROC-039
#[xmtp_common::test(unwrap_try = true)]
async fn exhausted_application_stream_preserves_worker_and_barrier_interests() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let topic = Topic::new_group_message(group.group_id);
    let (commands, receiver) = mpsc::unbounded_channel();
    let state = Arc::new(SharedState::default());
    let coordinator = Arc::new(IncomingCoordinator {
        commands,
        generations: AtomicU64::new(0),
        state: state.clone(),
    });
    let mut controller = Controller::new(alix.context.clone(), receiver, state);
    let application = coordinator.acquire_stream(IncomingScope::AllGroups);
    let worker = coordinator.acquire(IncomingScope::DeviceSyncGroups);
    let barrier = coordinator.acquire(IncomingScope::Barrier {
        targets: [(topic.clone(), Cursor(0))].into(),
        deadline: Instant::now() + Duration::from_secs(60),
        receive_policy: IncomingReceivePolicy::ImmediateQuery,
    });
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller.reconcile()?;
    controller.transport.recovery.failures = 10;
    controller.refresh_statuses();
    controller.retire_exhausted_streams();
    assert!(application.recovery_snapshot().terminal.is_some());
    assert!(!controller.scopes.contains_key(&application.id));
    assert!(worker.recovery_snapshot().terminal.is_none());
    assert!(barrier.recovery_snapshot().terminal.is_none());
    assert!(controller.scopes.contains_key(&worker.id));
    assert!(controller.scopes.contains_key(&barrier.id));
    assert!(controller.interested().contains(&topic));
    assert!(
        controller
            .interested()
            .contains(&Topic::new_welcome_message(alix.context.installation_id()))
    );

    // Updating an ended lease is not an application restart.
    application.replace_scope(IncomingScope::AllGroups);
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller.refresh_statuses();
    controller.retire_exhausted_streams();
    assert!(application.recovery_snapshot().terminal.is_some());
    assert!(!controller.scopes.contains_key(&application.id));

    let replacement = coordinator.acquire_stream(IncomingScope::AllGroups);
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller.reconcile()?;
    controller.refresh_statuses();
    controller.retire_exhausted_streams();
    assert!(replacement.recovery_snapshot().terminal.is_none());
    assert!(controller.scopes.contains_key(&replacement.id));
}
