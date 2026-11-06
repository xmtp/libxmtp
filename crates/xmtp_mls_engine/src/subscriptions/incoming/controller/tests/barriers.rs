//! Incoming controller tests: barriers.

use super::*;

// verifies: PROC-015
#[xmtp_common::test(unwrap_try = true)]
async fn a_second_scope_captures_a_fresh_target_on_the_shared_registration() {
    let mut context = context();
    Arc::get_mut(context.api_client.api_client.raw_mut_for_test().unwrap())
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

// verifies: PROC-015
#[xmtp_common::test(unwrap_try = true)]
async fn a_barrier_keeps_its_fixed_target_across_registration_and_target_replies() {
    let alix = EngineTestPeer::new().await;
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
    let alix = EngineTestPeer::new().await;
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
            envelopes: vec![ReceivedEnvelope::from(wire::ServerEnvelope {
                meta: Some(meta(&topic, 1)),
                envelope: Some(wire::ClientEnvelope::default()),
            })],
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
    let alix = EngineTestPeer::new().await;
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
            envelopes: vec![ReceivedEnvelope::from(wire::ServerEnvelope {
                meta: Some(meta(&topic, 10)),
                envelope: Some(wire::ClientEnvelope::default()),
            })],
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
            envelopes: vec![ReceivedEnvelope::from(wire::ServerEnvelope {
                meta: Some(meta(&topic, 20)),
                envelope: Some(wire::ClientEnvelope::default()),
            })],
        },
    ))));
    assert!(
        !controller.read_due(&topic, &key, started + interval * 2)?,
        "completed explicit sync and a caught-up healthy stream must not query again"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn suspension_blocks_live_queries_but_allows_an_explicit_barrier() {
    let alix = EngineTestPeer::new().await;
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
            envelopes: vec![ReceivedEnvelope::from(wire::ServerEnvelope {
                meta: Some(meta(&topic, 1)),
                envelope: Some(wire::ClientEnvelope::default()),
            })],
        },
    ))));
    assert!(
        !controller.read_due(&topic, &key, now)?,
        "completed explicit work must not resume suspended live polling"
    );
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
