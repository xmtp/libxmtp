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
            receive_policy: IncomingReceivePolicy::StreamFirst,
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
async fn stale_dependency_results_cannot_change_new_heads_or_requirements() {
    use crate::identity_updates::IdentityDependencyError;

    tester!(alix, disable_workers);
    for topic in [
        Topic::new_group_message(GroupId::generate()),
        Topic::new_identity_update(hex::decode(alix.inbox_id())?),
        Topic::new_welcome_message(alix.context.installation_id()),
    ] {
        let mut controller = controller(alix.context.clone());
        let key = topic_key(&topic)?;
        let after = controller.context.db().topic_progress(&key)?.received;
        let old = Cursor(after.0 + 10);
        let current = Cursor(after.0 + 20);
        controller.admit_received_batch(OrderedEnvelopeBatch {
            topic: topic.clone(),
            after,
            envelopes: [old, current]
                .into_iter()
                .map(|cursor| wire::ServerEnvelope {
                    meta: Some(meta(&topic, cursor.0)),
                    envelope: Some(wire::ClientEnvelope::default()),
                })
                .collect(),
        })?;
        controller.topics.entry(topic.clone()).or_default();
        controller
            .context
            .db()
            .complete_pending_envelope(&key, old)?;
        let requirement = IdentityRequirement {
            inbox_id: alix.inbox_id().to_string(),
            sequence_id: 1,
        };
        let next_requirement = IdentityRequirement {
            sequence_id: 2,
            ..requirement.clone()
        };
        let parent = |cursor| match topic.kind() {
            TopicKind::GroupMessagesV1 => DependencyParent::GroupHead(topic.clone(), cursor),
            TopicKind::IdentityUpdatesV1 => DependencyParent::IdentityHead(topic.clone(), cursor),
            _ => DependencyParent::Welcome(cursor),
        };
        for result in [
            Ok(()),
            Err(IdentityDependencyError::MissingReference(
                requirement.clone(),
            )),
            Err(IdentityDependencyError::InvalidSequence(0)),
        ] {
            // A Welcome can change its requirement without changing its cursor.
            let old_parent = if topic.kind() == TopicKind::WelcomeMessagesV1 {
                parent(current)
            } else {
                parent(old)
            };
            controller
                .dependency_registry
                .attach(old_parent, DependencyKey::Identity(requirement.clone()));
            controller.dependency_registry.attach(
                parent(current),
                DependencyKey::Identity(next_requirement.clone()),
            );
            controller.dependency_finished(DependencyResult::Identity(requirement.clone(), result));
            let pending = controller
                .context
                .db()
                .pending_envelope(&key, current)?
                .unwrap();
            assert!(!pending.blocked);
            assert_eq!(pending.retry_at_ns, 0);
            assert!(pending.error_code.is_none());
            assert!(controller.dependency_registry.contains(&parent(current)));
            let state = controller.topics.get(&topic).unwrap();
            assert!(state.error.is_none());
            assert!(state.processing.missing_reference.is_none());
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn passive_prefixes_do_not_limit_shared_identity_requests() {
    tester!(alix, disable_workers);
    let mut controller = controller(alix.context.clone());
    let limit = controller
        .context
        .incoming_runtime()
        .policy()
        .max_dependency_requests;
    for index in 0..limit {
        controller.dependency_registry.attach(
            DependencyParent::Welcome(Cursor(index as u64 + 10)),
            DependencyKey::GroupPrefix(GroupId::generate(), Cursor(1)),
        );
    }
    let requirement = IdentityRequirement {
        inbox_id: alix.inbox_id().to_string(),
        sequence_id: 0,
    };
    let group =
        DependencyParent::GroupHead(Topic::new_group_message(GroupId::generate()), Cursor(1));
    let welcome = DependencyParent::Welcome(Cursor(limit as u64 + 10));
    for parent in [group.clone(), welcome.clone()] {
        controller
            .dependency_registry
            .attach(parent, DependencyKey::Identity(requirement.clone()));
    }
    controller.start_dependencies();
    assert_eq!(controller.dependencies.len(), 1);
    assert_eq!(controller.dependency_registry.prefixes().count(), limit);
    let result = controller.dependencies.next().await.unwrap();
    assert!(matches!(
        result,
        DependencyResult::Identity(
            _,
            Err(crate::identity_updates::IdentityDependencyError::InvalidSequence(0))
        )
    ));
    assert_eq!(
        controller
            .dependency_registry
            .finish(&DependencyKey::Identity(requirement)),
        HashSet::from([group, welcome])
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn client_setup_selects_the_platform_transport_without_starting_it() {
    use xmtp_proto::api::HasStats;
    use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};

    tester!(seed, disable_workers);
    let api = Arc::new(crate::utils::DefaultTestClientCreator::create().build()?);
    #[cfg(not(target_arch = "wasm32"))]
    let _wires_before = crate::subscriptions::router_callbacks::shared_transport_count();
    // Registration uses an explicit barrier. Rebuild the registered identity
    // with a new API client to observe construction before its first interest.
    let alix = crate::builder::ClientBuilder::from_client(seed.client.clone())
        .api_client_with_streams(api)
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await?;
    let stats = alix.context.api().api_client.mls_stats();
    let coordinator = IncomingCoordinator::for_context(&alix.context);
    let empty = coordinator.acquire(IncomingScope::Topics(vec![]));
    xmtp_common::wait_for_eq(
        || async { empty.snapshot().processing },
        IncomingProcessing::Complete,
    )
    .await?;
    assert_eq!(stats.subscribe.get_count(), 0);
    assert_eq!(stats.subscribe_static.get_count(), 0);
    xmtp_common::if_native! { @
        assert_eq!(crate::subscriptions::router_callbacks::shared_transport_count(), _wires_before);
    }

    let topic = Topic::new_welcome_message(alix.context.installation_id());
    let first = coordinator.acquire(IncomingScope::Topics(vec![topic.clone()]));
    let second =
        IncomingCoordinator::for_context(&alix.context).acquire(IncomingScope::Topics(vec![topic]));
    for lease in [&first, &second] {
        xmtp_common::wait_for_eq(
            || async {
                lease
                    .snapshot()
                    .topics
                    .first()
                    .map(|topic| topic.registration)
            },
            Some(IncomingRegistration::Active),
        )
        .await?;
    }
    xmtp_common::if_native! { @
        assert_eq!(stats.subscribe.get_count(), 1);
        assert_eq!(stats.subscribe_static.get_count(), 0);
        assert_eq!(crate::subscriptions::router_callbacks::shared_transport_count(), _wires_before + 1);
    }
    xmtp_common::if_wasm! { @
        assert_eq!(stats.subscribe.get_count(), 0);
        assert_eq!(stats.subscribe_static.get_count(), 1);
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn releasing_readers_allows_recreation_and_releases_the_context() {
    tester!(alix, disable_workers);
    let weak_context = Arc::downgrade(&alix.context);
    let topic = Topic::new_welcome_message(alix.context.installation_id());
    for _ in 0..2 {
        let coordinator = IncomingCoordinator::for_context(&alix.context);
        let lease = coordinator.acquire(IncomingScope::Topics(vec![topic.clone()]));
        xmtp_common::wait_for_eq(
            || async {
                lease
                    .snapshot()
                    .topics
                    .first()
                    .map(|topic| topic.registration)
            },
            Some(IncomingRegistration::Active),
        )
        .await?;
        drop(lease);
        drop(coordinator);
        xmtp_common::wait_for_eq(
            || async { alix.context.incoming_runtime().coordinator.lock().is_none() },
            true,
        )
        .await?;
    }
    alix.close().await?;
    drop(alix);
    xmtp_common::wait_for_eq(|| async { weak_context.strong_count() }, 0).await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn replacing_the_api_with_query_only_setup_receives_and_processes_messages() {
    use crate::groups::send_message_opts::SendMessageOpts;
    use xmtp_proto::api::HasStats;
    use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = bo.create_group(None, None)?;
    group.add_members(&[alix.inbox_id()]).await?;
    alix.sync_welcomes().await?;
    let message_id = group
        .send_message(b"query-only receipt", SendMessageOpts::default())
        .await?;
    let api = Arc::new(crate::utils::DefaultTestClientCreator::create().build()?);
    let stats = api.mls_stats();
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(api)
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await?;
    stats.clear();
    let lease = IncomingCoordinator::for_context(&client.context)
        .acquire(IncomingScope::Groups(vec![group.group_id]));
    xmtp_common::wait_for_eq(
        || async { lease.snapshot().processing },
        IncomingProcessing::Complete,
    )
    .await?;
    let message = client.context.db().get_group_message(&message_id)?.unwrap();
    assert_eq!(message.decrypted_message_bytes, b"query-only receipt");
    assert!(stats.query_newest.get_count() > 0);
    assert!(stats.query.get_count() > 0);
    assert_eq!(stats.subscribe.get_count(), 0);
    assert_eq!(stats.subscribe_static.get_count(), 0);
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
    assert!(controller.transport.subscription().is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_barrier_keeps_its_fixed_target_across_registration_and_target_replies() {
    tester!(alix, disable_workers);
    let mut controller = controller(alix.context.clone());
    let topic = Topic::new_group_message(GroupId::generate());
    let deadline = Instant::now()
        + controller
            .context
            .incoming_runtime()
            .policy()
            .barrier_timeout;
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
            receive_policy: IncomingReceivePolicy::StreamFirst,
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
    assert!(controller.receipt(&topic).last_read.is_some());

    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    controller.registered([(topic.clone(), Cursor(0))].into());
    let settings = controller.context.incoming_runtime().policy().clone();
    let deadline = Instant::now() + settings.receiver_fallback_interval / 2;
    add_barrier_scope(&mut controller, 2, &topic, Cursor(1), deadline);
    controller.reconcile()?;
    controller.start_read();
    assert!(
        controller.read.is_some(),
        "the deadline cannot wait for the receiver interval"
    );
    let started = controller.receipt(&topic).last_read.unwrap();
    controller.start_read();
    assert_eq!(controller.receipt(&topic).last_read.unwrap(), started);
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
        .incoming_runtime()
        .policy()
        .receiver_fallback_interval;
    let deadline = Instant::now() + interval * 3;
    add_barrier_scope(&mut controller, 1, &topic, Cursor(20), deadline);
    controller.reconcile()?;
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    controller.registered([(topic.clone(), Cursor(20))].into());
    let started = controller.scopes[&1].receipt_wait_started;
    assert!(!controller.read_due(&topic, &key, started)?);

    controller.command(Command::Acquire {
        id: 2,
        scope: IncomingScope::Barrier {
            targets: [(topic.clone(), Cursor(20))].into(),
            deadline,
            receive_policy: IncomingReceivePolicy::ImmediateQuery,
        },
    });
    controller.reconcile()?;
    let query_started = controller.scopes[&2].receipt_wait_started;
    assert!(
        controller.read_due(&topic, &key, query_started)?,
        "explicit sync must not wait for a send on the same healthy stream"
    );
    controller.start_read();
    let result = controller.read.take().unwrap().await;
    controller.read_finished(result);
    assert_eq!(
        controller.context.db().topic_progress(&key)?.received,
        Cursor(0)
    );
    controller.command(Command::Release(2));
    controller.reconcile()?;
    assert!(
        !controller.read_due(&topic, &key, query_started)?,
        "releasing explicit sync must leave the send's original stream wait"
    );

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
    controller.scopes.get_mut(&1).unwrap().scope = ScopeKind::Topics(vec![topic.clone()]);
    assert!(!controller.read_due(&topic, &key, started + interval / 2)?);
    assert!(controller.read_due(&topic, &key, started + interval)?);
    controller.command(Command::Acquire {
        id: 3,
        scope: IncomingScope::Barrier {
            targets: [(topic.clone(), Cursor(20))].into(),
            deadline,
            receive_policy: IncomingReceivePolicy::ImmediateQuery,
        },
    });
    controller.reconcile()?;
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
        "completed explicit sync and a caught-up healthy stream must not query again"
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
    controller.transport.factory = Some(Arc::new(SuspendedFactory));
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

#[xmtp_common::test(unwrap_try = true)]
async fn a_welcome_failure_keeps_independent_pending_parents_runnable() {
    use crate::identity_updates::IdentityDependencyError;

    tester!(alix, disable_workers);
    let mut controller = controller(alix.context.clone());
    let topic = Topic::new_welcome_message(controller.context.installation_id());
    let key = topic_key(&topic)?;
    let deadline = Instant::now()
        + controller
            .context
            .incoming_runtime()
            .policy()
            .barrier_timeout;
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
    controller.dependency_registry.attach(
        DependencyParent::Welcome(Cursor(10)),
        DependencyKey::Identity(requirement.clone()),
    );
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
        IncomingError::Store(crate::mls_store::MlsStoreError::Api(
            xmtp_api::ApiError::InvalidResponse("cursor order"),
        )),
    );
    assert!(controller.receipt(&topic).blocked());

    controller.receive_error(
        topic.clone(),
        IncomingError::Store(crate::mls_store::MlsStoreError::Storage(
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
        IncomingError::Store(crate::mls_store::MlsStoreError::Api(
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
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
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

    // A retryable storage failure must also discard uncommitted transport progress.
    controller.receive_error(
        topic.clone(),
        crate::mls_store::MlsStoreError::Storage(
            xmtp_db::stream_storage::StreamStorageError::HeadChanged.into(),
        )
        .into(),
    );
    assert!(controller.transport.subscription().is_none());
    assert!(controller.transport.registered.is_empty());
    assert!(!controller.receipt(&topic).blocked());
    controller.refresh_statuses();
    assert_eq!(
        controller.state.statuses.lock()[&1].processing,
        IncomingProcessing::Pending
    );

    // Refuse a skipped prefix, but let this controller repair it from durable F.
    let missing = xmtp_db::stream_storage::StreamStorageError::MissingPrefix {
        after: 80,
        received: 20,
    };
    assert!(!missing.is_retryable());
    controller.receive_error(
        topic.clone(),
        crate::mls_store::MlsStoreError::Storage(missing.into()).into(),
    );
    assert!(!controller.receipt(&topic).blocked());
    controller.refresh_statuses();
    let snapshot = controller.state.statuses.lock()[&1].clone();
    assert_eq!(snapshot.processing, IncomingProcessing::Pending);
    assert!(snapshot.topics[0].error.as_ref().unwrap().is_retryable());
    assert_eq!(snapshot.topics[0].received, Cursor(20));
    assert_eq!(snapshot.topics[0].processed, Cursor(0));
    controller.start_open();
    assert!(
        !controller.transport.is_opening(),
        "topic reconciliation preserves backoff"
    );
    controller.read_queue.push_back(topic.clone());
    controller
        .topics
        .entry(topic.clone())
        .or_default()
        .receipt
        .last_read = None;
    controller.start_read();
    assert!(
        controller.read.is_some(),
        "the missing prefix can be queried"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn each_kind_keeps_its_budget_and_only_committed_chunks_are_acknowledged() {
    use xmtp_db::{TransactionOutcome, XmtpMlsStorageProvider};

    tester!(alix, disable_workers);
    let mut settings = alix.context.incoming_runtime().policy().clone();
    settings.max_fetched_rows = 8;
    settings.max_fetched_bytes = 4096;
    settings.max_admission_rows = 1;
    settings.max_admission_bytes = 1024;
    settings.group_pending.rows = 2;
    settings.welcome_pending.bytes = 1;
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .stream_policy(settings)
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
    controller.transport.requested = [topic.clone(), welcome.clone()].into();
    assert_eq!(controller.fetched_limits().max_rows, 8);
    assert_eq!(controller.fetched_limits().max_bytes, 4096);
    let acknowledged = Arc::new(Mutex::new(Vec::new()));
    let observed = acknowledged.clone();
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
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
    assert!(controller.receipt(&topic).paused);
    assert!(!controller.receipt(&welcome).paused);
    assert_eq!(controller.transport.permanent_failures, 0);
    assert!(controller.transport.subscription().is_none());

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
    assert!(!controller.receipt(&topic).paused);
    let observed = acknowledged.clone();
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        move |cursors| observed.lock().push(cursors),
    ));
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
    assert!(!controller.receipt(&topic).blocked());

    // Run the production loop with one retained head and a legal eight-row batch.
    // The fake transport exposes each new registration and never replays on its own.
    use xmtp_common::{StreamHandle, wait_for_eq};
    use xmtp_db::incoming_envelope::IncomingRetry;

    tester!(receiver, disable_workers);
    let group = receiver.create_group(None, None)?;
    let mut settings = receiver.context.incoming_runtime().policy().clone();
    settings.max_fetched_rows = 8;
    settings.max_admission_rows = 8;
    settings.max_pending_rows_per_topic = 8;
    settings.active_database_poll_interval = Duration::from_millis(10);
    settings.receiver_fallback_interval = Duration::from_millis(100);
    let client = crate::builder::ClientBuilder::from_client(receiver.client.clone())
        .stream_policy(settings)
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await?;
    let topic = Topic::new_group_message(group.group_id);
    let key = topic_key(&topic)?;
    let before = client.context.db().topic_progress(&key)?;
    let retained = Cursor(before.received.0 + 10);
    let replayed = Cursor(retained.0 + 8);
    let later = Cursor(replayed.0 + 1);
    let envelope = |cursor: Cursor| wire::ServerEnvelope {
        meta: Some(meta(&topic, cursor.0)),
        envelope: Some(wire::ClientEnvelope {
            payload: Some(wire::client_envelope::Payload::GroupMessage(
                wire::GroupMessage {
                    data: vec![0, 1, 0],
                    ..Default::default()
                },
            )),
        }),
    };
    let limits = client
        .context
        .incoming_runtime()
        .policy()
        .incoming_limits(NetworkEntityKind::Group);
    MlsStore::new(client.context.clone()).admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: before.received,
            envelopes: vec![envelope(retained)],
        },
        limits,
    )?;
    client.context.db().set_incoming_retry(
        &key,
        retained,
        &IncomingRetry {
            retry_at_ns: xmtp_common::time::now_ns() + 30 * xmtp_common::NS_IN_SEC,
            blocked: false,
            error_code: Some("identity_dependency".into()),
            retry_expires_at_ns: None,
        },
    )?;
    let (commands, receiver) = mpsc::unbounded_channel();
    let state = Arc::new(SharedState::default());
    let mut running = Controller::new(client.context.clone(), receiver, state.clone());
    add_scope(&mut running, 1, &topic);
    let (opened, mut registrations) = mpsc::unbounded_channel();
    let acknowledged = Arc::new(Mutex::new(Vec::new()));
    let observed = acknowledged.clone();
    let acknowledged_topic = topic.clone();
    running.transport.factory = Some(Arc::new(
        move |cursors: TopicCursor, limits: IncomingBatchLimits| -> SubscriptionFuture {
            assert_eq!(limits.max_rows, 8);
            let (send, receive) = mpsc::unbounded_channel();
            opened.send((cursors, send)).unwrap();
            let observed = observed.clone();
            let topic = acknowledged_topic.clone();
            Box::pin(async move {
                let events = futures::stream::unfold(receive, |mut receive| async move {
                    receive.recv().await.map(|event| (event, receive))
                });
                Ok(IncomingSubscription::new(
                    Box::pin(events),
                    move |cursors| {
                        if let Some(received) = cursors.get(&topic) {
                            observed.lock().push(*received);
                        }
                    },
                ))
            })
        },
    ));
    let task = xmtp_common::spawn(None, running.run());
    let (starts, first) = xmtp_common::time::timeout(Duration::from_secs(5), registrations.recv())
        .await?
        .unwrap();
    assert_eq!(starts[&topic], retained);
    first.send(Ok(IncomingEvent::Registered {
        starts,
        targets: [(topic.clone(), replayed)].into(),
    }))?;
    let batch = OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: retained,
        envelopes: (1..=8)
            .map(|offset| envelope(Cursor(retained.0 + offset)))
            .collect(),
    };
    first.send(Ok(IncomingEvent::OrderedBatch(batch.clone())))?;
    xmtp_common::time::timeout(Duration::from_secs(5), first.closed()).await?;
    assert_eq!(client.context.db().topic_progress(&key)?.received, retained);
    assert_eq!(
        client.context.db().topic_progress(&key)?.processed,
        before.processed
    );
    assert_eq!(
        client
            .context
            .db()
            .pending_states_through(&key, later)?
            .len(),
        1
    );
    assert!(acknowledged.lock().is_empty());

    // Make the retained head runnable. The same controller frees capacity and reopens.
    client.context.db().set_incoming_retry(
        &key,
        retained,
        &IncomingRetry {
            retry_at_ns: 0,
            blocked: false,
            error_code: None,
            retry_expires_at_ns: None,
        },
    )?;
    wait_for_eq(
        || async { client.context.db().topic_progress(&key).unwrap().processed },
        retained,
    )
    .await?;
    let (starts, resumed) =
        xmtp_common::time::timeout(Duration::from_secs(5), registrations.recv())
            .await?
            .unwrap();
    assert_eq!(starts[&topic], retained);
    resumed.send(Ok(IncomingEvent::Registered {
        starts,
        targets: [(topic.clone(), later)].into(),
    }))?;
    resumed.send(Ok(IncomingEvent::OrderedBatch(batch)))?;
    wait_for_eq(
        || async { client.context.db().topic_progress(&key).unwrap().processed },
        replayed,
    )
    .await?;
    resumed.send(Ok(IncomingEvent::OrderedBatch(OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: replayed,
        envelopes: vec![envelope(later)],
    })))?;
    wait_for_eq(
        || async { client.context.db().topic_progress(&key).unwrap().processed },
        later,
    )
    .await?;
    assert_eq!(client.context.db().topic_progress(&key)?.received, later);
    assert!(
        client
            .context
            .db()
            .pending_states_through(&key, later)?
            .is_empty()
    );
    assert_eq!(*acknowledged.lock(), vec![replayed, later]);
    let snapshot = state.statuses.lock()[&1].clone();
    assert_eq!(snapshot.scope_generation, 1);
    assert_eq!(snapshot.connection_generation, 2);
    assert_eq!(snapshot.topics[0].target, Some(replayed));
    assert_ne!(snapshot.processing, IncomingProcessing::Blocked);
    commands.send(Command::Release(1))?;
    task.join().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn byte_chunks_validate_the_complete_input_before_receipt() {
    tester!(alix, disable_workers);
    let topic = Topic::new_group_message(GroupId::generate());
    let envelope = |sequence| wire::ServerEnvelope {
        meta: Some(meta(&topic, sequence)),
        envelope: Some(wire::ClientEnvelope::default()),
    };
    let mut settings = alix.context.incoming_runtime().policy().clone();
    settings.max_admission_rows = 8;
    settings.max_admission_bytes = envelope(10).encoded_len() as u64;
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .stream_policy(settings)
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
    caro.create_group(None, None)?
        .send_message(b"sequence gap", SendMessageOpts::default())
        .await?;
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
    assert!(target.0 > before.received.0 + 2);
    let mut reused = envelopes[0].envelope.clone().unwrap();
    let mut tampered = envelopes[0].clone();
    tampered.meta.as_mut().unwrap().cursor = Some(wire::Cursor {
        sequence_id: before.received.0 + 2,
    });
    let Some(wire::client_envelope::Payload::GroupMessage(message)) =
        tampered.envelope.as_mut().unwrap().payload.as_mut()
    else {
        panic!("group payload");
    };
    *message.data.last_mut().unwrap() ^= 1;
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
            envelopes: [invalid, tampered].into_iter().chain(envelopes).collect(),
        },
    ))));
    assert!(controller.process_ready());
    assert_eq!(
        bo.context.db().topic_progress(&key)?.processed,
        Cursor(before.received.0 + 1)
    );
    let authenticator = bo_group.epoch_authenticator().await?;
    let GroupHeadOutcome::Progress {
        cursor,
        result: Err(error),
    } = bo_group.process_pending_group_head(None)?
    else {
        panic!("tampered ciphertext must be rejected before its valid generation is consumed");
    };
    assert_eq!(cursor, Cursor(before.received.0 + 2));
    let inner = match &error {
        GroupMessageProcessingError::OpenMlsProcessMessage(error)
        | GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
            ProcessMessageWithAppDataError::OpenMls(error),
        ) => error,
        other => panic!("expected an OpenMLS decryption error, got {other:?}"),
    };
    assert!(matches!(
        inner,
        ProcessMessageError::ValidationError(ValidationError::UnableToDecrypt(_))
    ));
    assert!(!matches!(
        inner,
        ProcessMessageError::ValidationError(ValidationError::UnableToDecrypt(
            MessageDecryptionError::SecretTreeError(SecretTreeError::SecretReuseError)
        ))
    ));
    assert_eq!(bo_group.epoch_authenticator().await?, authenticator);
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
    assert_eq!(
        bo_group
            .find_messages(&MsgQueryArgs::default())?
            .iter()
            .filter(|message| message.decrypted_message_bytes == b"after the rejected head")
            .count(),
        1
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
