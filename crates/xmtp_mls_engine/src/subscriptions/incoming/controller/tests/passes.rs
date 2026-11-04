//! Incoming controller tests: passes.

use super::*;

/// Each admission point after the first turn takes at most 32 waiting events
/// and yields: here the one after 32 turns and the one before the status
/// rebuild, while the first point finds nothing waiting.
#[cfg(not(target_arch = "wasm32"))] // counts tokio scheduler turns
#[xmtp_common::test(unwrap_try = true)]
async fn each_admission_point_takes_waiting_events_and_yields() {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        task::Poll,
    };

    let alix = EngineTestPeer::new().await;
    let busy = stored_groups(&alix.context, TURNS_BETWEEN_ADMISSIONS + 1);
    let mut controller = controller(alix.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::Topics(busy.clone()),
    });
    register_all(&mut controller);
    // The run loop's first pass publishes the connection.
    controller.run_pass(false).await;
    for topic in &busy {
        admit_empty(&mut controller, topic, &[10]);
    }
    // Nothing waits at the first poll; then more registrations wait than two
    // admission points may take, so an unbounded drain fails instead of hanging.
    let mut polls = 0;
    let events = futures::stream::poll_fn(move |_| {
        polls += 1;
        if polls == 1 || polls > 3 * MAX_EVENTS_PER_ADMISSION + 2 {
            return Poll::Pending;
        }
        Poll::Ready(Some(Ok(IncomingEvent::Registered {
            starts: TopicCursor::new(),
            targets: [(Topic::new_group_message(GroupId::generate()), Cursor(0))].into(),
        })))
    });
    controller.transport.state =
        TransportState::Streaming(IncomingSubscription::new(Box::pin(events), |_| {}));
    let registered = controller.transport.registered.len();
    // This task runs once each time the pass yields.
    let yields = Arc::new(AtomicUsize::new(0));
    let _counter = xmtp_common::spawn(None, {
        let yields = yields.clone();
        async move {
            loop {
                yields.fetch_add(1, Ordering::SeqCst);
                xmtp_common::task::yield_now().await;
            }
        }
    });
    // Unconstrained: a tokio resource that runs out of its cooperative budget
    // cannot add a yield, so only the admission points yield.
    tokio::task::unconstrained(controller.run_pass(false)).await;
    assert_eq!(
        controller.transport.registered.len() - registered,
        2 * MAX_EVENTS_PER_ADMISSION
    );
    assert_eq!(yields.load(Ordering::SeqCst), 3);
}

/// A reader that processing wakes runs before the status rebuild of the same
/// pass, so a fresh message does not wait for the rebuild.
#[xmtp_common::test(unwrap_try = true)]
async fn a_woken_reader_runs_before_the_status_rebuild() {
    use crate::groups::send_message_opts::SendMessageOpts;
    use xmtp_common::StreamHandle;
    use xmtp_mls_core::events::InternalEvent;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.find_groups(GroupQueryArgs::default())?.remove(0);
    bo_group.receive().await?;
    let topic = Topic::new_group_message(bo_group.group_id);
    let before = bo.context.db().topic_progress(&topic_key(&topic)?)?;
    alix_group
        .send_message(b"wakes the reader", SendMessageOpts::default())
        .await?;
    let envelopes = bo
        .context
        .api()
        .query_all(
            [(topic.clone(), before.received)].into(),
            bo.context.api().limits().max_query_limit as u32,
        )
        .await?;
    let (coordinator, mut controller) = coordinated_controller(bo.context.clone());
    let lease = Arc::new(coordinator.acquire(IncomingScope::Topics(vec![topic.clone()])));
    register_all(&mut controller);
    controller.admit_received_batch(OrderedEnvelopeBatch {
        topic,
        after: before.received,
        envelopes: envelopes.into_iter().map(Into::into).collect(),
    })?;
    controller.refresh_statuses();
    let processed = |lease: &IncomingLease| lease.snapshot().topics[0].processed;
    assert_eq!(processed(&lease), before.processed);

    let stored = bo.context.events().subscribe(
        xmtp_events::EventFilter::default()
            .with_internal(|event| matches!(event, InternalEvent::MessageStored { .. })),
        Some(4),
    );
    let reader = xmtp_common::spawn(None, {
        let lease = lease.clone();
        async move {
            stored.next().await;
            processed(&lease)
        }
    });
    controller.run_pass(false).await;
    assert_eq!(
        reader.join().await?,
        before.processed,
        "the reader ran after the status rebuild"
    );
    assert!(processed(&lease) > before.processed);
}

/// While 1,000 groups catch up, another connection unblocks held heads and
/// delivers new Welcomes. Every envelope is handled once, in order, and none
/// is left pending.
// verifies: PROC-005, PROC-013
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread")]
#[cfg_attr(target_arch = "wasm32", ignore)] // the writer needs another thread
async fn released_heads_and_new_welcomes_finish_during_a_catch_up() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use xmtp_common::StreamHandle;
    use xmtp_db::ConnectionExt;

    const GROUPS: usize = 1_000;
    const HELD: usize = 100;
    const WELCOMES: u64 = 20;

    #[derive(diesel::QueryableByName)]
    struct Completed {
        #[diesel(sql_type = diesel::sql_types::Binary)]
        entity_id: Vec<u8>,
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        sequence_id: i64,
    }

    let alix = EngineTestPeer::persistent().await;
    // Every completed envelope is deleted once; log the deletions in order.
    run_raw_sql(
        &alix.context,
        "CREATE TABLE completed (entity_id BLOB, sequence_id BIGINT)",
    );
    run_raw_sql(
        &alix.context,
        "CREATE TRIGGER log_completion AFTER DELETE ON incoming_envelopes \
         BEGIN INSERT INTO completed VALUES (OLD.entity_id, OLD.sequence_id); END",
    );
    let groups = local_groups(&alix.context, GROUPS);
    let welcome = Topic::new_welcome_message(alix.context.installation_id());
    let mut controller = controller(alix.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::Topics(groups.iter().cloned().chain([welcome.clone()]).collect()),
    });
    register_all(&mut controller);
    for topic in &groups {
        admit_empty(&mut controller, topic, &[10, 20]);
    }
    let held = groups[..HELD].to_vec();
    for topic in &held {
        retry_at(&alix.context, topic, 10, 0, true);
        // This controller has already rechecked the blocked head.
        controller
            .topics
            .entry(topic.clone())
            .or_default()
            .processing
            .retried_head = Some(Cursor(10));
    }

    let written = Arc::new(AtomicBool::new(false));
    let writer = xmtp_common::spawn(None, {
        let (context, welcome, written) = (alix.context.clone(), welcome.clone(), written.clone());
        async move {
            let limits = context
                .incoming_runtime()
                .policy()
                .incoming_limits(NetworkEntityKind::Welcome);
            let store = MlsStore::new(context.clone());
            for (index, topic) in held.iter().enumerate() {
                retry_at(&context, topic, 10, 0, false);
                let sequence = index as u64 + 1;
                if sequence <= WELCOMES {
                    let batch = OrderedEnvelopeBatch {
                        topic: welcome.clone(),
                        after: Cursor(sequence - 1),
                        envelopes: vec![empty_envelope(&welcome, sequence)],
                    };
                    store.admit_incoming_batch(&batch, limits).unwrap();
                }
                xmtp_common::time::sleep(Duration::from_millis(1)).await;
            }
            written.store(true, Ordering::SeqCst);
        }
    });

    let db = alix.context.db();
    let pending = || -> bool {
        [NetworkEntityKind::Group, NetworkEntityKind::Welcome]
            .into_iter()
            .any(|kind| !db.pending_topic_usage(kind).unwrap().is_empty())
    };
    let drained = xmtp_common::time::timeout(Duration::from_secs(60), async {
        while !written.load(Ordering::SeqCst) || pending() {
            controller.run_pass(false).await;
        }
    })
    .await;
    assert!(drained.is_ok(), "work was left pending");
    writer.join().await?;

    let completed = db.raw_query(|conn| {
        use diesel::RunQueryDsl;
        diesel::sql_query("SELECT entity_id, sequence_id FROM completed ORDER BY rowid")
            .load::<Completed>(conn)
    })?;
    let mut by_topic: HashMap<Vec<u8>, Vec<i64>> = HashMap::new();
    for row in completed {
        by_topic
            .entry(row.entity_id)
            .or_default()
            .push(row.sequence_id);
    }
    for topic in &groups {
        assert_eq!(by_topic[topic.identifier()], [10, 20]);
    }
    let welcomes: Vec<_> = (1..=WELCOMES as i64).collect();
    assert_eq!(by_topic[welcome.identifier()], welcomes);
    assert_eq!(by_topic.len(), GROUPS + 1);
}

/// A pass admits a batch that was already waiting and processes it in the
/// same pass.
#[xmtp_common::test(unwrap_try = true)]
async fn a_pass_processes_a_batch_that_was_already_waiting() {
    let alix = EngineTestPeer::new().await;
    let topic = local_groups(&alix.context, 1).remove(0);
    let mut controller = controller(alix.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::Topics(vec![topic.clone()]),
    });
    register_all(&mut controller);
    // The run loop's first pass publishes the connection.
    controller.run_pass(false).await;
    let batch = OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: Cursor(0),
        envelopes: vec![empty_envelope(&topic, 10)],
    };
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(
            futures::stream::iter([Ok(IncomingEvent::OrderedBatch(batch))])
                .chain(futures::stream::pending()),
        ),
        |_| {},
    ));
    controller.run_pass(false).await;
    assert_eq!(
        alix.context
            .db()
            .topic_progress(&topic_key(&topic)?)?
            .processed,
        Cursor(10)
    );
}

/// A batch taken at the last admission point, just before the status rebuild,
/// gets the next pass at once instead of waiting the poll interval.
#[xmtp_common::test(unwrap_try = true)]
async fn a_batch_taken_before_the_rebuild_gets_the_next_pass_at_once() {
    use std::task::Poll;

    let alix = EngineTestPeer::new().await;
    let topic = local_groups(&alix.context, 1).remove(0);
    let mut controller = controller(alix.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::Topics(vec![topic.clone()]),
    });
    register_all(&mut controller);
    // The run loop's first pass publishes the connection.
    controller.run_pass(false).await;
    // The batch is not ready at the first admission point, only at the last.
    let mut batch = Some(OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: Cursor(0),
        envelopes: vec![empty_envelope(&topic, 10)],
    });
    let mut polls = 0;
    let events = futures::stream::poll_fn(move |_| {
        polls += 1;
        match batch.take() {
            Some(batch) if polls > 1 => Poll::Ready(Some(Ok(IncomingEvent::OrderedBatch(batch)))),
            unready => {
                batch = unready;
                Poll::Pending
            }
        }
    });
    controller.transport.state =
        TransportState::Streaming(IncomingSubscription::new(Box::pin(events), |_| {}));
    let key = topic_key(&topic)?;
    let db = alix.context.db();
    assert!(
        controller.run_pass(false).await,
        "the admitted batch needs a pass"
    );
    assert_eq!(db.topic_progress(&key)?.received, Cursor(10));
    controller.run_pass(false).await;
    assert_eq!(db.topic_progress(&key)?.processed, Cursor(10));
}

/// When the ready-heads read fails, every queued topic takes its turn, so each
/// one reports its own read error.
#[xmtp_common::test(unwrap_try = true)]
async fn a_failed_heads_read_reports_each_topics_error() {
    let alix = EngineTestPeer::new().await;
    let topic = stored_groups(&alix.context, 1).remove(0);
    let mut controller = controller(alix.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::Topics(vec![topic.clone()]),
    });
    controller.reconcile()?;
    // A topic with pending rows, so the heads read runs.
    admit_empty(&mut controller, &topic, &[10]);
    run_raw_sql(
        &alix.context,
        "ALTER TABLE incoming_envelopes RENAME TO incoming_envelopes_hidden",
    );
    controller.process_ready(&mut false).await;
    assert!(matches!(
        controller.topics[&topic].error.as_deref(),
        Some(IncomingError::Storage(_))
    ));
}

/// Record each statement the test store runs from now on; it has one
/// connection.
fn record_statements<C: XmtpSharedContext>(context: &C) -> Arc<std::sync::Mutex<Vec<String>>> {
    use xmtp_db::{
        ConnectionExt,
        diesel::{Connection, connection::InstrumentationEvent},
    };
    let executed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorder = executed.clone();
    context
        .db()
        .raw_query(|conn| {
            conn.set_instrumentation(move |event: InstrumentationEvent<'_>| {
                if let InstrumentationEvent::StartQuery { query, .. } = event {
                    recorder.lock().unwrap().push(query.to_string());
                }
            });
            Ok(())
        })
        .unwrap();
    executed
}

fn heads_reads(executed: &std::sync::Mutex<Vec<String>>) -> Vec<String> {
    executed
        .lock()
        .unwrap()
        .iter()
        .filter(|statement| {
            statement.contains("json_each") && statement.contains("MIN(sequence_id)")
        })
        .cloned()
        .collect()
}

/// The heads read covers only the queued topics with pending rows, so the idle
/// topics of a large scope cost nothing in a pass.
#[xmtp_common::test(unwrap_try = true)]
async fn the_heads_read_covers_only_topics_with_pending_rows() {
    let alix = EngineTestPeer::new().await;
    let idle = stored_groups(&alix.context, 10);
    let busy = local_groups(&alix.context, 1).remove(0);
    let mut controller = controller(alix.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::Topics(idle.iter().cloned().chain([busy.clone()]).collect()),
    });
    register_all(&mut controller);
    admit_empty(&mut controller, &busy, &[10]);
    let executed = record_statements(&alix.context);
    controller.run_pass(false).await;
    let heads = heads_reads(&executed);
    assert_eq!(heads.len(), 1, "{heads:?}");
    assert!(heads[0].contains(&hex::encode(busy.identifier())));
    for topic in &idle {
        assert!(!heads[0].contains(&hex::encode(topic.identifier())));
    }
}

/// A failed reconcile leaves no list of topics with pending rows, so the pass
/// reads every queued head, including one that another writer stored.
#[xmtp_common::test(unwrap_try = true)]
async fn a_failed_reconcile_reads_every_queued_head() {
    let alix = EngineTestPeer::new().await;
    let topic = stored_groups(&alix.context, 1).remove(0);
    let mut controller = controller(alix.context.clone());
    controller.command(Command::Acquire {
        id: 1,
        scope: IncomingScope::Topics(vec![topic.clone()]),
    });
    register_all(&mut controller);
    controller.run_pass(false).await;
    let limits = alix
        .context
        .incoming_runtime()
        .policy()
        .incoming_limits(NetworkEntityKind::Group);
    MlsStore::new(alix.context.clone()).admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: Cursor(0),
            envelopes: vec![empty_envelope(&topic, 10)],
        },
        limits,
    )?;
    run_raw_sql(
        &alix.context,
        "ALTER TABLE group_set_generation RENAME TO group_set_generation_hidden",
    );
    let executed = record_statements(&alix.context);
    controller.run_pass(false).await;
    assert!(controller.storage_error.is_some());
    let heads = heads_reads(&executed);
    assert_eq!(heads.len(), 1, "{heads:?}");
    assert!(heads[0].contains(&hex::encode(topic.identifier())));
}

fn scope_received<C: XmtpSharedContext>(controller: &Controller<C>) -> Cursor {
    controller.state.statuses.lock()[&1].topics[0].received
}

/// A follow-up pass with no new work keeps the status the previous pass
/// published; the pass after any other wake rebuilds it.
#[xmtp_common::test(unwrap_try = true)]
async fn a_follow_up_pass_without_work_skips_the_status_rebuild() {
    let alix = EngineTestPeer::new().await;
    let group = local_groups(&alix.context, 1).remove(0);
    let mut controller = controller(alix.context.clone());
    add_scope(&mut controller, 1, &group);
    register_all(&mut controller);
    controller.run_pass(false).await;
    assert_eq!(scope_received(&controller), Cursor(0));
    // Another connection records receipt.
    run_raw_sql(
        &alix.context,
        &format!(
            "INSERT OR REPLACE INTO refresh_state \
             (entity_id, entity_kind, sequence_id, received_sequence_id) \
             VALUES (X'{}', 2, 0, 50)",
            hex::encode(group.identifier())
        ),
    );
    controller.run_pass(true).await;
    assert_eq!(scope_received(&controller), Cursor(0));
    controller.run_pass(false).await;
    assert_eq!(scope_received(&controller), Cursor(50));
}

/// A follow-up pass rebuilds status when it makes progress, or when it admits
/// events even without progress.
#[xmtp_common::test(unwrap_try = true)]
async fn a_follow_up_pass_with_work_rebuilds_status() {
    use std::task::Poll;

    let alix = EngineTestPeer::new().await;
    let group = local_groups(&alix.context, 1).remove(0);
    let mut controller = controller(alix.context.clone());
    add_scope(&mut controller, 1, &group);
    register_all(&mut controller);
    controller.run_pass(false).await;
    // Progress: the pass processes a row admitted before it.
    admit_empty(&mut controller, &group, &[10]);
    assert!(controller.run_pass(true).await);
    assert_eq!(scope_received(&controller), Cursor(10));
    // Admission alone: a batch waits only at the pre-rebuild admission point.
    let mut batch = Some(OrderedEnvelopeBatch {
        topic: group.clone(),
        after: Cursor(10),
        envelopes: vec![empty_envelope(&group, 20)],
    });
    let mut polls = 0;
    let events = futures::stream::poll_fn(move |_| {
        polls += 1;
        match batch.take() {
            Some(batch) if polls > 1 => Poll::Ready(Some(Ok(IncomingEvent::OrderedBatch(batch)))),
            unready => {
                batch = unready;
                Poll::Pending
            }
        }
    });
    controller.transport.state =
        TransportState::Streaming(IncomingSubscription::new(Box::pin(events), |_| {}));
    assert!(controller.run_pass(true).await);
    assert_eq!(scope_received(&controller), Cursor(20));
}

/// A follow-up pass with no new work still publishes a connection state that
/// changed after the previous rebuild, so the app sees the transition.
// verifies: EVENT-027
#[xmtp_common::test(unwrap_try = true)]
async fn a_follow_up_pass_publishes_a_changed_connection() {
    let alix = EngineTestPeer::new().await;
    let groups = local_groups(&alix.context, 2);
    let mut controller = controller(alix.context.clone());
    add_scope(&mut controller, 1, &groups[0]);
    register_all(&mut controller);
    controller.run_pass(false).await;
    let published = |controller: &Controller<_>| controller.state.statuses.lock()[&1].connection;
    assert_eq!(published(&controller), IncomingConnection::Connected);
    // New interest makes a pass replace the requested topics after it
    // published status, as `start_open` does at the end of a pass. No open
    // starts here, so none can finish before the follow-up pass.
    add_scope(&mut controller, 2, &groups[1]);
    controller.reconcile()?;
    let topics = controller.interested();
    controller.transport.request(topics);
    let changed = controller.transport.connection();
    assert_ne!(changed, IncomingConnection::Connected);
    controller.run_pass(true).await;
    assert_eq!(published(&controller), changed);
}

/// An admission point takes no transport event while a connection change is
/// unpublished, so an open whose stream drops at once publishes each state.
// verifies: EVENT-027
#[xmtp_common::test(unwrap_try = true)]
async fn each_connection_change_is_published_before_the_next_event() {
    let alix = EngineTestPeer::new().await;
    let group = local_groups(&alix.context, 1).remove(0);
    let mut controller = controller(alix.context.clone());
    add_scope(&mut controller, 1, &group);
    register_all(&mut controller);
    controller.run_pass(false).await;
    // A reconnect whose open succeeds and whose stream then drops at once.
    let dropped = IncomingSubscription::new(
        Box::pin(futures::stream::iter([Ok(IncomingEvent::Disconnected)])),
        |_| {},
    );
    controller.transport.generation = 2;
    controller.transport.state =
        TransportState::Opening(Box::pin(async move { Ok(Opened::Stream(dropped)) }));
    let mut published = Vec::new();
    for _ in 0..3 {
        controller.run_pass(false).await;
        published.push(controller.state.statuses.lock()[&1].connection);
    }
    assert_eq!(
        published,
        [
            IncomingConnection::Reconnecting,
            IncomingConnection::Connected,
            IncomingConnection::Reconnecting,
        ]
    );
}
