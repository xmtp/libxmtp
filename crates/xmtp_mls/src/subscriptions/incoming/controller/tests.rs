use super::*;
use crate::{test::mock::context, tester};
use xmtp_common::Generate;
use xmtp_proto::{
    backend_v1 as wire,
    types::{GroupId, OrderedEnvelopeBatch},
};

fn controller<C: XmtpSharedContext + 'static>(context: C) -> Controller<C> {
    let (_, receiver) = mpsc::unbounded_channel();
    Controller::new(context, receiver, Arc::new(SharedState::default()))
}

fn add_scope<C: XmtpSharedContext + 'static>(
    controller: &mut Controller<C>,
    id: u64,
    topic: &Topic,
) {
    controller
        .state
        .statuses
        .lock()
        .insert(id, IncomingStatus::pending(id));
    controller.command(Command::Acquire {
        id,
        scope: IncomingScope::Topics(vec![topic.clone()]),
    });
    controller
        .scopes
        .get_mut(&id)
        .unwrap()
        .topics
        .insert(topic.clone());
}

fn add_barrier_scope<C: XmtpSharedContext + 'static>(
    controller: &mut Controller<C>,
    id: u64,
    topic: &Topic,
    target: Cursor,
    deadline: Instant,
) {
    controller
        .state
        .statuses
        .lock()
        .insert(id, IncomingStatus::pending(id));
    controller.command(Command::Acquire {
        id,
        scope: IncomingScope::Barrier {
            targets: [(topic.clone(), target)].into(),
            deadline,
        },
    });
}

fn meta(topic: &Topic, sequence_id: u64) -> wire::EnvelopeMeta {
    wire::EnvelopeMeta {
        cursor: Some(wire::Cursor { sequence_id }),
        server_ns: 123,
        topic: Some(wire::Topic {
            topic: topic.cloned_vec(),
        }),
        message_hash: Some(wire::MessageHash {
            hash: Some(wire::message_hash::Hash::Sha256(vec![7; 32])),
        }),
        ..Default::default()
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn reopening_after_the_last_release_keeps_the_controller_alive() {
    let context = context();
    let (commands, receiver) = mpsc::unbounded_channel();
    let state = Arc::new(SharedState::default());
    let coordinator = Arc::new(IncomingCoordinator {
        commands,
        generations: AtomicU64::new(0),
        transport_mode: Mutex::new(TransportMode::Unary),
        state: state.clone(),
    });
    *context.incoming_coordinator().lock() = Some(coordinator.clone());
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
    assert!(controller.context.incoming_coordinator().lock().is_none());
    assert!(controller.commands.is_closed());
}

/// Transport setup stays alive until the first reader acquires its lease.
#[xmtp_common::test(unwrap_try = true)]
fn a_selected_transport_handle_keeps_its_factory_until_the_first_lease() {
    let context = Arc::new(context());
    let (commands, receiver) = mpsc::unbounded_channel();
    let state = Arc::new(SharedState::default());
    let coordinator = Arc::new(IncomingCoordinator {
        commands,
        generations: AtomicU64::new(0),
        transport_mode: Mutex::new(TransportMode::Unary),
        state: state.clone(),
    });
    *context.incoming_coordinator().lock() = Some(coordinator.clone());
    let mut controller = Controller::new(context.clone(), receiver, state);
    let selected = IncomingCoordinator::enable_stream_transport(&context);
    drop(coordinator);

    let factory = controller.commands.try_recv()?;
    assert!(matches!(&factory, Command::SetFactory(_)));
    controller.command(factory);
    assert!(controller.commands.is_empty());
    assert!(controller.scopes.is_empty());
    assert!(!controller.stop_if_idle());

    let reader = IncomingCoordinator::for_context(&context);
    assert!(Arc::ptr_eq(&reader, &selected));
    let lease = reader.acquire(IncomingScope::Topics(vec![]));
    drop(reader);
    drop(selected);
    let acquire = controller.commands.try_recv()?;
    controller.command(acquire);
    assert!(!controller.stop_if_idle());
    assert!(controller.factory.is_some());

    drop(lease);
    let release = controller.commands.try_recv()?;
    controller.command(release);
    assert!(controller.stop_if_idle());
    assert!(context.incoming_coordinator().lock().is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_second_scope_captures_a_fresh_target_on_the_shared_registration() {
    let mut context = context();
    Arc::get_mut(&mut context.api_client.api_client)
        .unwrap()
        .expect_query_newest()
        .times(1)
        .returning(|request| {
            assert!(!request.include_full_envelope);
            let topic = Topic::parse(&request.topics[0].topic).unwrap();
            Ok(wire::QueryNewestResponse {
                results: vec![wire::query_newest_response::Result {
                    topic: Some(request.topics[0].clone()),
                    meta: Some(meta(&topic, 90)),
                    envelope: None,
                }],
            })
        });
    let mut controller = controller(Arc::new(context));
    let topic = Topic::new_group_message(GroupId::generate());
    add_scope(&mut controller, 1, &topic);
    controller.registered([(topic.clone(), Cursor(40))].into());
    add_scope(&mut controller, 2, &topic);
    assert!(!controller.scopes[&2].targets.contains_key(&topic));

    controller.start_targets();
    let result = controller.targets.take().unwrap().await;
    controller.targets_finished(result);
    assert_eq!(controller.scopes[&1].targets[&topic], Cursor(40));
    assert_eq!(controller.scopes[&2].targets[&topic], Cursor(90));
    assert!(controller.subscription.is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_barrier_keeps_its_fixed_target_across_registration_and_target_replies() {
    tester!(alix, disable_workers);
    let mut controller = controller(alix.context.clone());
    let topic = Topic::new_group_message(GroupId::generate());
    let deadline = Instant::now() + controller.context.stream_settings().barrier_timeout;
    add_barrier_scope(&mut controller, 1, &topic, Cursor(40), deadline);
    controller.reconcile()?;
    assert!(controller.scopes[&1].topics.contains(&topic));
    controller.registered([(topic.clone(), Cursor(90))].into());
    controller.targets_finished((vec![(1, 1)], Ok([(topic.clone(), Cursor(120))].into())));
    controller.refresh_statuses();
    assert_eq!(
        controller.state.statuses.lock()[&1].topics[0].target,
        Some(Cursor(40))
    );
    controller.start_targets();
    assert!(controller.targets.is_none());

    controller.command(Command::Replace {
        id: 1,
        generation: 2,
        scope: IncomingScope::Barrier {
            targets: [(topic.clone(), Cursor(60))].into(),
            deadline,
        },
    });
    controller.reconcile()?;
    controller.registered([(topic.clone(), Cursor(150))].into());
    controller.targets_finished((vec![(1, 2)], Ok([(topic.clone(), Cursor(180))].into())));
    assert_eq!(controller.scopes[&1].targets[&topic], Cursor(60));
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_new_barrier_after_an_empty_query_starts_deadline_fallback() {
    tester!(alix, disable_workers);
    let mut controller = controller(alix.context.clone());
    let topic = Topic::new_group_message(GroupId::generate());
    let key = topic_key(&topic)?;
    add_scope(&mut controller, 1, &topic);
    controller.reconcile()?;
    controller.start_read();
    let result = controller.read.take().unwrap().await;
    controller.read_finished(result);
    assert_eq!(
        controller.context.db().topic_progress(&key)?.received,
        Cursor(0)
    );
    assert!(controller.last_read.contains_key(&topic));

    controller.subscription = Some(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    controller.connection = IncomingConnection::Connected;
    controller.registered([(topic.clone(), Cursor(0))].into());
    let settings = controller.context.stream_settings().clone();
    let deadline = Instant::now() + settings.receiver_fallback_interval / 2;
    add_barrier_scope(&mut controller, 2, &topic, Cursor(1), deadline);
    controller.reconcile()?;
    controller.start_read();
    assert!(
        controller.read.is_some(),
        "the deadline cannot wait for the receiver interval"
    );
    let started = controller.last_read[&topic];
    controller.start_read();
    assert_eq!(controller.last_read[&topic], started);
    let result = controller.read.take().unwrap().await;
    controller.read_finished(result);
    assert!(!controller.read_due(
        &topic,
        &key,
        started + settings.active_database_poll_interval / 2,
    )?);
    assert!(controller.read_due(
        &topic,
        &key,
        started + settings.active_database_poll_interval,
    )?);

    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(
        OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: Cursor(0),
            envelopes: vec![wire::ServerEnvelope {
                meta: Some(meta(&topic, 1)),
                envelope: Some(wire::ClientEnvelope::default()),
            }],
        },
    ))));
    controller.command(Command::Release(1));
    controller.reconcile()?;
    assert!(
        !controller.read_due(&topic, &key, started + settings.receiver_fallback_interval,)?,
        "a barrier with its full received prefix needs processing only"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_healthy_receiver_gets_one_fixed_barrier_wait() {
    tester!(alix, disable_workers);
    let mut controller = controller(alix.context.clone());
    let topic = Topic::new_group_message(GroupId::generate());
    let key = topic_key(&topic)?;
    let interval = controller
        .context
        .stream_settings()
        .receiver_fallback_interval;
    let deadline = Instant::now() + interval * 3;
    add_barrier_scope(&mut controller, 1, &topic, Cursor(20), deadline);
    controller.reconcile()?;
    controller.subscription = Some(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    controller.connection = IncomingConnection::Connected;
    controller.registered([(topic.clone(), Cursor(20))].into());
    let started = controller.scopes[&1].receipt_wait_started;
    assert!(!controller.read_due(&topic, &key, started)?);
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(
        OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: Cursor(0),
            envelopes: vec![wire::ServerEnvelope {
                meta: Some(meta(&topic, 10)),
                envelope: Some(wire::ClientEnvelope::default()),
            }],
        },
    ))));
    assert!(!controller.read_due(&topic, &key, started + interval / 2)?);
    assert!(controller.read_due(&topic, &key, started + interval)?);
    controller.scopes.get_mut(&1).unwrap().scope = IncomingScope::Topics(vec![topic.clone()]);
    assert!(!controller.read_due(&topic, &key, started + interval / 2)?);
    assert!(controller.read_due(&topic, &key, started + interval)?);
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(
        OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: Cursor(10),
            envelopes: vec![wire::ServerEnvelope {
                meta: Some(meta(&topic, 20)),
                envelope: Some(wire::ClientEnvelope::default()),
            }],
        },
    ))));
    assert!(
        !controller.read_due(&topic, &key, started + interval * 2)?,
        "a caught-up healthy live stream must not poll the network"
    );
}

struct SuspendedFactory;

impl SubscriptionFactory for SuspendedFactory {
    fn open(&self, _: TopicCursor, _: IncomingBatchLimits) -> SubscriptionFuture {
        panic!("the read-policy test must not open a transport");
    }

    fn is_suspended(&self) -> bool {
        true
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn suspension_blocks_live_queries_but_allows_an_explicit_barrier() {
    tester!(alix, disable_workers);
    let mut controller = controller(alix.context.clone());
    controller.factory = Some(Arc::new(SuspendedFactory));
    let topic = Topic::new_group_message(GroupId::generate());
    let key = topic_key(&topic)?;
    add_scope(&mut controller, 1, &topic);
    controller.reconcile()?;
    let now = Instant::now();
    assert!(!controller.read_due(&topic, &key, now)?);
    controller.start_read();
    controller.start_targets();
    assert!(controller.read.is_none());
    assert!(controller.targets.is_none());

    add_barrier_scope(
        &mut controller,
        2,
        &topic,
        Cursor(1),
        now + Duration::from_secs(1),
    );
    controller.reconcile()?;
    assert!(controller.read_due(&topic, &key, now)?);
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(
        OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: Cursor(0),
            envelopes: vec![wire::ServerEnvelope {
                meta: Some(meta(&topic, 1)),
                envelope: Some(wire::ClientEnvelope::default()),
            }],
        },
    ))));
    assert!(
        !controller.read_due(&topic, &key, now)?,
        "completed explicit work must not resume suspended live polling"
    );
}

/// Suspended receipt follows only parents within the fixed Welcome target.
#[xmtp_common::test(unwrap_try = true)]
async fn suspended_welcome_barrier_receives_only_its_required_group_prefixes() {
    tester!(alix, disable_workers);
    let mut controller = controller(alix.context.clone());
    controller.factory = Some(Arc::new(SuspendedFactory));
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
    controller.welcome_prefixes.extend([
        (Cursor(30), (group_id, Cursor(10))),
        (Cursor(35), (group_id, Cursor(15))),
        (Cursor(50), (group_id, Cursor(20))),
        (
            Cursor(60),
            (GroupId::try_from(later.identifier())?, Cursor(20)),
        ),
    ]);
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
                envelopes: vec![wire::ServerEnvelope {
                    meta: Some(meta(&topic, received)),
                    envelope: Some(wire::ClientEnvelope::default()),
                }],
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
    tester!(alix, disable_workers);
    let mut controller = controller(alix.context.clone());
    controller.factory = Some(Arc::new(SuspendedFactory));
    let group_id = GroupId::generate();
    let topic = Topic::new_group_message(group_id);
    let key = topic_key(&topic)?;
    let welcome = Topic::new_welcome_message(alix.context.installation_id());
    let settings = controller.context.stream_settings().clone();
    let now = Instant::now();
    add_barrier_scope(
        &mut controller,
        1,
        &welcome,
        Cursor(40),
        now + settings.receiver_fallback_interval * 3,
    );
    controller
        .welcome_prefixes
        .insert(Cursor(30), (group_id, Cursor(10)));
    controller.reconcile()?;
    controller.subscription = Some(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    controller.connection = IncomingConnection::Connected;
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
    controller.last_read.insert(topic.clone(), urgent);
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

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_stream_setup_keeps_bidi_as_the_last_factory() {
    use crate::subscriptions::router_callbacks::{resume_bidi_streams, suspend_bidi_streams};
    tester!(alix, disable_workers);
    let (commands, mut receiver) = mpsc::unbounded_channel();
    let coordinator = Arc::new(IncomingCoordinator {
        commands,
        generations: AtomicU64::new(0),
        transport_mode: Mutex::new(TransportMode::Unary),
        state: Arc::new(SharedState::default()),
    });
    *alix.context.incoming_coordinator().lock() = Some(coordinator.clone());
    suspend_bidi_streams().await?;
    let runtime = tokio::runtime::Handle::current();
    let gate = std::sync::Barrier::new(2);
    std::thread::scope(|threads| {
        threads.spawn(|| {
            let _entered = runtime.enter();
            gate.wait();
            let _handle = IncomingCoordinator::enable_stream_transport(&alix.context);
        });
        threads.spawn(|| {
            let _entered = runtime.enter();
            gate.wait();
            let _handle = IncomingCoordinator::enable_bidi_transport(&alix.context);
        });
    });
    let mut last_factory = None;
    while let Ok(Command::SetFactory(factory)) = receiver.try_recv() {
        last_factory = Some(factory);
    }
    assert!(
        last_factory.unwrap().is_suspended(),
        "bidi must be the final selected factory"
    );
    assert!(*coordinator.transport_mode.lock() == TransportMode::Bidi);
    let _handle = IncomingCoordinator::enable_stream_transport(&alix.context);
    assert!(
        receiver.try_recv().is_err(),
        "later generic setup must not downgrade bidi"
    );
    resume_bidi_streams().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_welcome_failure_keeps_independent_pending_parents_runnable() {
    use crate::identity_updates::IdentityDependencyError;

    tester!(alix, disable_workers);
    let mut controller = controller(alix.context.clone());
    let topic = Topic::new_welcome_message(controller.context.installation_id());
    let key = topic_key(&topic)?;
    let deadline = Instant::now() + controller.context.stream_settings().barrier_timeout;
    add_barrier_scope(&mut controller, 1, &topic, Cursor(30), deadline);
    controller.reconcile()?;
    controller.registered([(topic.clone(), Cursor(30))].into());
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(
        OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: Cursor(0),
            envelopes: [10, 20]
                .into_iter()
                .map(|sequence| wire::ServerEnvelope {
                    meta: Some(meta(&topic, sequence)),
                    envelope: Some(wire::ClientEnvelope::default()),
                })
                .collect(),
        },
    ))));
    let requirement = IdentityRequirement {
        inbox_id: controller.context.inbox_id().to_string(),
        sequence_id: 0,
    };
    controller
        .welcome_identity
        .insert(Cursor(10), requirement.clone());
    controller.dependency_finished(DependencyResult::Identity(
        requirement,
        Err(IdentityDependencyError::InvalidSequence(0)),
    ));
    controller.refresh_statuses();
    let status = controller.state.statuses.lock()[&1].topics[0].clone();
    assert_eq!(status.unresolved_welcomes, 2);
    assert_eq!(status.processing, IncomingProcessing::Pending);
    assert!(status.error.is_some_and(|error| !error.is_retryable()));

    controller.source_error(NetworkError::new(xmtp_api::ApiError::InvalidResponse(
        "cursor order",
    )));
    controller.refresh_statuses();
    let snapshot = controller.state.statuses.lock()[&1].clone();
    assert_eq!(snapshot.connection, IncomingConnection::Failed);
    assert_eq!(snapshot.processing, IncomingProcessing::Pending);
    assert_eq!(snapshot.topics[0].processing, IncomingProcessing::Pending);

    controller
        .context
        .db()
        .complete_pending_envelope(&key, Cursor(20))?;
    controller.refresh_statuses();
    let snapshot = controller.state.statuses.lock()[&1].clone();
    assert_eq!(snapshot.processing, IncomingProcessing::Blocked);
    assert_eq!(snapshot.topics[0].unresolved_welcomes, 1);
    assert_eq!(snapshot.topics[0].received, Cursor(20));
}

#[xmtp_common::test(unwrap_try = true)]
fn a_replaced_scope_ignores_an_older_target_request() {
    let mut controller = controller(context());
    let topic = Topic::new_group_message(GroupId::generate());
    add_scope(&mut controller, 1, &topic);
    controller.command(Command::Replace {
        id: 1,
        generation: 2,
        scope: IncomingScope::Topics(vec![topic.clone()]),
    });
    controller
        .scopes
        .get_mut(&1)
        .unwrap()
        .topics
        .insert(topic.clone());
    controller.targets_finished((vec![(1, 1)], Ok([(topic.clone(), Cursor(90))].into())));
    assert!(!controller.scopes[&1].targets.contains_key(&topic));
}

#[xmtp_common::test(unwrap_try = true)]
fn permanent_source_and_topic_errors_stop_automatic_receipt() {
    let mut controller = controller(context());
    let topic = Topic::new_group_message(GroupId::generate());
    controller.read_queue.push_back(topic.clone());
    controller.receive_error(
        topic.clone(),
        IncomingError::Store(crate::mls_store::MlsStoreError::Api(
            xmtp_api::ApiError::Envelope(xmtp_api_backend::envelope::EnvelopeError::Capacity),
        )),
    );
    controller.start_read();
    assert!(controller.read.is_none());
    assert!(controller.receive_blocked.contains(&topic));

    controller.receive_blocked.clear();
    controller.source_error(NetworkError::new(xmtp_api::ApiError::InvalidResponse(
        "cursor order",
    )));
    controller.start_read();
    assert!(controller.read.is_none());
    assert_eq!(controller.connection, IncomingConnection::Failed);
}

#[xmtp_common::test(unwrap_try = true)]
async fn receipt_acknowledgement_follows_storage_and_never_uses_the_target() {
    tester!(alix, disable_workers);
    let topic = Topic::new_group_message(GroupId::generate());
    let mut controller = controller(alix.context.clone());
    add_scope(&mut controller, 1, &topic);
    controller.registered([(topic.clone(), Cursor(90))].into());
    controller.refresh_statuses();
    assert_eq!(
        controller.state.statuses.lock()[&1].topics[0].received,
        Cursor(0)
    );

    let acknowledged = Arc::new(Mutex::new(Vec::new()));
    let observed = acknowledged.clone();
    controller.subscription = Some(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        move |cursors| observed.lock().push(cursors),
    ));
    let batch = OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: Cursor(0),
        envelopes: [10, 20]
            .into_iter()
            .map(|sequence| wire::ServerEnvelope {
                meta: Some(meta(&topic, sequence)),
                envelope: Some(wire::ClientEnvelope::default()),
            })
            .collect(),
    };
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(batch.clone()))));
    controller.refresh_statuses();
    let status = controller.state.statuses.lock()[&1].topics[0].clone();
    assert_eq!(status.received, Cursor(20));
    assert_eq!(status.processed, Cursor(0));
    assert_eq!(status.processing, IncomingProcessing::Pending);
    assert_eq!(acknowledged.lock()[0][&topic], Cursor(20));
    // A replay after an unacknowledged delivery has no duplicate durable rows.
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(batch))));
    let pending = alix
        .context
        .db()
        .pending_states_through(&topic_key(&topic)?, Cursor(90))?;
    assert_eq!(pending.len(), 2);
    assert_eq!(acknowledged.lock().len(), 2);
}

#[xmtp_common::test(unwrap_try = true)]
async fn each_kind_keeps_its_budget_and_only_committed_chunks_are_acknowledged() {
    use xmtp_db::{TransactionOutcome, XmtpMlsStorageProvider};

    tester!(alix, disable_workers);
    let mut settings = alix.context.stream_settings().clone();
    settings.max_fetched_rows = 8;
    settings.max_fetched_bytes = 4096;
    settings.max_admission_rows = 1;
    settings.max_admission_bytes = 1024;
    settings.group_pending.rows = 2;
    settings.welcome_pending.bytes = 1;
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .stream_settings(settings)
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await?;
    let topic = Topic::new_group_message(GroupId::generate());
    let welcome = Topic::new_welcome_message(client.installation_public_key());
    let key = topic_key(&topic)?;
    let mut controller = controller(client.context.clone());
    add_scope(&mut controller, 1, &topic);
    add_scope(&mut controller, 2, &welcome);
    controller.subscribed = [topic.clone(), welcome.clone()].into();
    assert_eq!(controller.fetched_limits().max_rows, 8);
    assert_eq!(controller.fetched_limits().max_bytes, 4096);
    let acknowledged = Arc::new(Mutex::new(Vec::new()));
    let observed = acknowledged.clone();
    controller.subscription = Some(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        move |cursors| observed.lock().push(cursors),
    ));
    let batch = OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: Cursor(0),
        envelopes: [10, 20, 30]
            .into_iter()
            .map(|sequence| wire::ServerEnvelope {
                meta: Some(meta(&topic, sequence)),
                envelope: Some(wire::ClientEnvelope::default()),
            })
            .collect(),
    };
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(batch.clone()))));
    assert_eq!(
        client.context.db().topic_progress(&key)?.received,
        Cursor(20)
    );
    assert_eq!(acknowledged.lock().len(), 2);
    assert_eq!(acknowledged.lock()[0][&topic], Cursor(10));
    assert_eq!(acknowledged.lock()[1][&topic], Cursor(20));
    assert!(controller.paused.contains(&topic));
    assert!(!controller.paused.contains(&welcome));
    assert!(!controller.source_failed);

    crate::state_tx::state_write(client.context.mls_storage(), |tx| {
        let storage = tx.storage();
        for sequence in [10, 20] {
            storage
                .db()
                .complete_pending_envelope(&key, Cursor(sequence))?;
        }
        Ok::<_, xmtp_db::StorageError>(TransactionOutcome::Continue(()))
    })?;
    controller.reconcile()?;
    assert!(!controller.paused.contains(&topic));
    // An overlapping replay must use each chunk's wire start, not its durable receipt.
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(batch))));
    assert_eq!(
        client.context.db().topic_progress(&key)?.received,
        Cursor(30)
    );
    assert_eq!(acknowledged.lock().last().unwrap()[&topic], Cursor(30));
    assert_eq!(
        client
            .context
            .db()
            .pending_states_through(&key, Cursor(30))?
            .len(),
        1
    );
    assert!(!controller.receive_blocked.contains(&topic));
}

#[xmtp_common::test(unwrap_try = true)]
async fn byte_chunks_validate_the_complete_input_before_receipt() {
    tester!(alix, disable_workers);
    let topic = Topic::new_group_message(GroupId::generate());
    let envelope = |sequence| wire::ServerEnvelope {
        meta: Some(meta(&topic, sequence)),
        envelope: Some(wire::ClientEnvelope::default()),
    };
    let mut settings = alix.context.stream_settings().clone();
    settings.max_admission_rows = 8;
    settings.max_admission_bytes = envelope(10).encoded_len() as u64;
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .stream_settings(settings)
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await?;
    let mut controller = controller(client.context.clone());
    let key = topic_key(&topic)?;
    assert!(
        controller
            .admit_received_batch(OrderedEnvelopeBatch {
                topic: topic.clone(),
                after: Cursor(0),
                envelopes: vec![envelope(10), envelope(5)],
            })
            .is_err()
    );
    assert_eq!(
        client.context.db().topic_progress(&key)?.received,
        Cursor(0)
    );
    controller.admit_received_batch(OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: Cursor(0),
        envelopes: vec![envelope(10), envelope(20)],
    })?;
    assert_eq!(
        client.context.db().topic_progress(&key)?.received,
        Cursor(20)
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn replacement_cancels_old_obligations_and_retains_notifications() {
    let (commands, _receiver) = mpsc::unbounded_channel();
    let coordinator = Arc::new(IncomingCoordinator {
        commands,
        generations: AtomicU64::new(0),
        transport_mode: Mutex::new(TransportMode::Unary),
        state: Arc::new(SharedState::default()),
    });
    let lease = coordinator.acquire(IncomingScope::Topics(vec![]));
    let old = lease.replace_topics(vec![Topic::new_group_message(GroupId::generate())]);
    assert_eq!(old.processing, IncomingProcessing::Cancelled);
    let current = lease.snapshot();
    assert!(current.scope_generation > old.scope_generation);
    assert_eq!(
        current.previous.unwrap().processing,
        IncomingProcessing::Cancelled
    );
    xmtp_common::time::timeout(Duration::from_secs(1), lease.changed()).await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn an_invalid_supported_head_does_not_hold_a_later_valid_message() {
    use crate::groups::{
        app_data::ProcessMessageWithAppDataError,
        mls_sync::{GroupHeadOutcome, GroupMessageProcessingError},
        send_message_opts::SendMessageOpts,
    };
    use openmls::{
        framing::errors::{MessageDecryptionError, SecretTreeError},
        group::{ProcessMessageError, ValidationError},
    };
    use xmtp_api::PublishUnit;
    use xmtp_db::group_message::MsgQueryArgs;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.find_groups(GroupQueryArgs::default())?.remove(0);
    bo_group.receive().await?;
    let topic = Topic::new_group_message(bo_group.group_id);
    let key = topic_key(&topic)?;
    let before = bo.context.db().topic_progress(&key)?;
    let message_count = bo_group.find_messages(&MsgQueryArgs::default())?.len();
    // Independent traffic leaves a sparse sequence for the malformed head.
    tester!(caro, disable_workers);
    assert_ne!(caro.inbox_id(), bo.inbox_id());
    alix_group
        .send_message(b"after the rejected head", SendMessageOpts::default())
        .await?;
    let envelopes = bo
        .context
        .api()
        .query_all([(topic.clone(), before.received)].into(), 100)
        .await?;
    assert_eq!(envelopes.len(), 1);
    let target = Cursor(
        envelopes[0]
            .meta
            .as_ref()
            .unwrap()
            .cursor
            .as_ref()
            .unwrap()
            .sequence_id,
    );
    assert!(target.0 > before.received.0 + 1);
    let mut reused = envelopes[0].envelope.clone().unwrap();
    let mut invalid = envelopes[0].clone();
    invalid.meta.as_mut().unwrap().cursor = Some(wire::Cursor {
        sequence_id: before.received.0 + 1,
    });
    let Some(wire::client_envelope::Payload::GroupMessage(message)) =
        invalid.envelope.as_mut().unwrap().payload.as_mut()
    else {
        panic!("group payload");
    };
    message.data = vec![0, 1, 0];
    let mut controller = controller(bo.context.clone());
    add_scope(&mut controller, 1, &topic);
    controller.reconcile()?;
    controller.registered([(topic.clone(), target)].into());
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(
        OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: before.received,
            envelopes: std::iter::once(invalid).chain(envelopes).collect(),
        },
    ))));
    assert!(controller.process_ready());
    assert_eq!(
        bo.context.db().topic_progress(&key)?.processed,
        Cursor(before.received.0 + 1)
    );
    assert!(controller.process_ready());
    controller.refresh_statuses();
    assert_eq!(
        controller.state.statuses.lock()[&1].processing,
        IncomingProcessing::Complete
    );
    assert_eq!(bo.context.db().topic_progress(&key)?.processed, target);
    assert_eq!(
        bo_group.find_messages(&MsgQueryArgs::default())?.len(),
        message_count + 1
    );

    // Publish real ciphertext after its generation was consumed. Keep its TLS shape.
    let Some(wire::client_envelope::Payload::GroupMessage(message)) = &mut reused.payload else {
        panic!("group payload");
    };
    *message.data.last_mut().unwrap() ^= 1;
    let receipts = alix
        .context
        .api()
        .publish_units(vec![PublishUnit::single(reused)?])
        .await?;
    let (_, reused_cursor, _) = xmtp_api_backend::envelope::metadata(&receipts[0], topic.kind())?;
    let epoch_before_rejection = bo_group.epoch_authenticator().await?;
    alix_group
        .update_group_name("after the reused generation".into())
        .await?;
    let rows = bo
        .context
        .api()
        .query_all([(topic.clone(), target)].into(), 100)
        .await?;
    assert_eq!(rows.len(), 2);
    let (_, commit_cursor, _) = xmtp_api_backend::envelope::metadata(
        rows.last().unwrap().meta.as_ref().unwrap(),
        topic.kind(),
    )?;
    controller.registered([(topic.clone(), commit_cursor)].into());
    controller.admit_received_batch(OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: target,
        envelopes: rows,
    })?;
    let outcome = bo_group.process_pending_group_head(None)?;
    let GroupHeadOutcome::Progress {
        cursor,
        result: Err(error),
    } = outcome
    else {
        panic!("reused generation must reach terminal rejection");
    };
    assert_eq!(cursor, reused_cursor);
    let inner = match &error {
        GroupMessageProcessingError::OpenMlsProcessMessage(error)
        | GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
            ProcessMessageWithAppDataError::OpenMls(error),
        ) => error,
        other => panic!("expected an OpenMLS decryption error, got {other:?}"),
    };
    assert!(
        matches!(
            inner,
            ProcessMessageError::ValidationError(ValidationError::UnableToDecrypt(
                MessageDecryptionError::SecretTreeError(SecretTreeError::SecretReuseError)
            ))
        ),
        "expected a consumed generation, got {inner:?}"
    );
    assert_eq!(
        bo_group.epoch_authenticator().await?,
        epoch_before_rejection
    );
    assert_eq!(
        bo.context.db().topic_progress(&key)?.processed,
        reused_cursor
    );
    assert_eq!(
        bo.context
            .db()
            .read_last_rejection(&key)?
            .unwrap()
            .sequence_id,
        reused_cursor
    );
    assert!(controller.process_ready());
    assert_eq!(
        bo.context.db().topic_progress(&key)?.processed,
        commit_cursor
    );
    assert_eq!(bo_group.group_name()?, "after the reused generation");
    assert_eq!(
        bo_group.epoch_authenticator().await?,
        alix_group.epoch_authenticator().await?
    );
    alix_group
        .send_message(b"after the reused generation", SendMessageOpts::default())
        .await?;
    bo_group.receive().await?;
    assert_eq!(
        bo_group
            .find_messages(&MsgQueryArgs::default())?
            .iter()
            .filter(|message| message.decrypted_message_bytes == b"after the reused generation")
            .count(),
        1
    );
}
