//! Bidirectional stream fuzz tests: transport delivery.

use super::*;

fn wm_cursor(message: &ReceivedEnvelope) -> Option<u64> {
    Some(message.meta()?.cursor.as_ref()?.sequence_id)
}

/// A random resume floor: the beginning of time, roughly mid-history,
/// exactly the latest cursor, or past the live edge.
fn random_floor(rng: &mut StdRng, latest: u64) -> u64 {
    match rng.random_range(0..4u8) {
        0 => 0,
        1 => latest / 2,
        2 => latest,
        _ => latest + 10_000,
    }
}

/// What one lease's collector saw, in arrival order.
#[derive(Default)]
struct Collected {
    ids: HashMap<Topic, Vec<u64>>,
    catch_ups: usize,
    ended: bool,
}

/// One lease in a recovery chain.
struct Link {
    chain: usize,
    /// Index of the consumer (transport) this lease lives on.
    consumer: usize,
    floors: HashMap<Topic, u64>,
    state: Arc<std::sync::Mutex<Collected>>,
    /// Millis the collector sleeps before its next poll — the stall lever
    /// that provokes the transport's backpressure drop.
    stall_ms: Arc<AtomicU64>,
    /// Dropping this unsubscribes deliberately (no recovery obligation).
    die: Option<oneshot::Sender<()>>,
    /// A deliberate unsubscribe, as opposed to a transport-side drop.
    killed: bool,
}

#[allow(clippy::too_many_arguments)]
fn spawn_collector(
    mut lease: TopicLease<BackendBinding>,
    state: Arc<std::sync::Mutex<Collected>>,
    stall_ms: Arc<AtomicU64>,
    mut die: oneshot::Receiver<()>,
    ends: mpsc::UnboundedSender<usize>,
    index: usize,
    welcome_topic: Topic,
    latest: Arc<AtomicU64>,
) {
    xmtp_common::spawn(None, async move {
        loop {
            let stall = stall_ms.swap(0, Ordering::Relaxed);
            if stall > 0 {
                xmtp_common::time::sleep(Duration::from_millis(stall)).await;
            }
            tokio::select! {
                _ = &mut die => {
                    state.lock().expect("checker lock").ended = true;
                    return; // dropping the lease derefs its topics
                }
                event = lease.next() => match event {
                    None => {
                        state.lock().expect("checker lock").ended = true;
                        let _ = ends.send(index);
                        return;
                    }
                    Some(LeaseEvent::GroupMessages(batch)) => {
                        let mut state = state.lock().expect("checker lock");
                        for message in &batch {
                            if let Some((topic, id)) = gm_parts(message) {
                                latest.fetch_max(id, Ordering::Relaxed);
                                state.ids.entry(topic).or_default().push(id);
                            }
                        }
                    }
                    Some(LeaseEvent::WelcomeMessages(batch)) => {
                        let mut state = state.lock().expect("checker lock");
                        for message in &batch {
                            if let Some(id) = wm_cursor(message) {
                                state
                                    .ids
                                    .entry(welcome_topic.clone())
                                    .or_default()
                                    .push(id);
                            }
                        }
                    }
                    Some(LeaseEvent::CatchUpComplete) => {
                        state.lock().expect("checker lock").catch_ups += 1;
                    }
                },
            }
        }
    });
}

/// Client delivery-contract fuzz: random lease/yank/drop/stall/suspend
/// schedules — plus real TCP faults, via toxiproxy — on up to ten real
/// [`BidiTransport`]s (one per proxied subscriber client), while up to ten
/// member clients race publishes into the same groups, with per-lease
/// strictly-increasing delivery and chain-union completeness checked
/// against the server's own record.
///
/// Subscriber wires share the local backend proxy. Each fault interrupts all
/// subscribers; every transport must reconnect and retain its lease floors.
/// Publishers and verification queries use direct connections. Run proxy tests
/// serially in a separate nextest invocation to prevent shared fault changes.
#[xmtp_common::timeout(Duration::from_secs(300))]
#[xmtp_common::test(unwrap_try = true)]
async fn fuzz_transport_delivery_never_loses_above_the_floor() {
    xmtp_common::toxiproxy_test(async || {
        fuzz_transport_delivery(fuzz_seed(), fuzz_rounds(48)).await;
    })
    .await;
}

async fn fuzz_transport_delivery(seed: u64, rounds: usize) {
    let mut rng = StdRng::seed_from_u64(seed);

    tester!(alix);
    let n_producers = rng.random_range(2..=10usize);
    let n_consumers = rng.random_range(2..=10usize);
    tracing::info!(
        "bidi transport fuzz: seed={seed} rounds={rounds} \
         producers={n_producers} consumers={n_consumers}"
    );
    let mut producers = Vec::new();
    for i in 0..n_producers {
        producers.push(
            TesterBuilder::new()
                .with_name(&format!("xport_prod_{i}"))
                .build()
                .await,
        );
    }
    // Each consumer has its own transport and Welcome topic. All subscriber
    // connections use the same proxy; MLS membership is not needed for reads.
    let mut consumers = Vec::new();
    for i in 0..n_consumers {
        consumers.push(
            TesterBuilder::new()
                .with_name(&format!("xport_sub_{i}"))
                .proxy()
                .build()
                .await,
        );
    }
    let member_ids: Vec<_> = producers.iter().map(|p| p.inbox_id()).collect();
    let initial_groups = 4usize;
    let mut groups = Vec::new();
    for _ in 0..initial_groups {
        let group = alix
            .create_group_with_members(&member_ids, None, None)
            .await
            .expect("create_group_with_members failed");
        let topic = Topic::new_group_message(group.group_id);
        groups.push((group, topic));
    }
    for (group, _) in &groups {
        for msg_idx in 0..rng.random_range(6..14usize) {
            publish_message(group, format!("seed {msg_idx}").as_bytes(), seed).await;
        }
    }
    // As in the server-contract fuzz: settle each producer's one-time
    // post-join KeyUpdate commit serially, so racing bursts hit a stable
    // epoch.
    let group_ids: Vec<_> = groups.iter().map(|(group, _)| group.group_id).collect();
    let mut producer_groups = Vec::new();
    for producer in &producers {
        let handles = joined_groups(producer, &group_ids).await;
        for handle in &handles {
            publish_message(handle, b"settle", seed).await;
        }
        producer_groups.push(handles);
    }

    let api = alix.context.api();
    let mut transports: Vec<BidiTransport<BackendBinding>> = Vec::new();
    let mut welcome_topics = Vec::new();
    for consumer in &consumers {
        let api = consumer.context.api().api_client.raw_for_test().clone();
        transports.push(BidiTransport::new(
            move |initial| {
                let api = api.clone();
                async move {
                    BidiConnection::open(&api, initial)
                        .await
                        .map_err(OpenError::new)
                }
            },
            false,
        ));
        // Each consumer's welcome topic is fuzzed like any other: its
        // leases may include it, and a fuzz op mints real welcomes by
        // creating groups WITH random consumers. Its own id space, so its
        // own floor palette.
        welcome_topics.push(Topic::new_welcome_message(
            consumer.context.installation_id(),
        ));
    }

    let (ends_tx, mut ends_rx) = mpsc::unbounded_channel::<usize>();
    let mut links: Vec<Link> = Vec::new();
    // chain id -> the chain's ORIGINAL floors (the durable ask the whole
    // recovery chain must eventually satisfy). The consumer a chain lives
    // on rides along in its links.
    let mut chains: HashMap<usize, HashMap<Topic, u64>> = HashMap::new();
    // The REAL group-message high-water, folded by every collector — random
    // floors derive from it, so mid-history and at-the-edge floors actually
    // mean what they say (a fabricated estimate would collapse them all to
    // "beginning of time" against the shared node's global sequence).
    let latest = Arc::new(AtomicU64::new(0));
    let mut suspended = vec![false; n_consumers];
    let mut resume_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    let mut bursts: JoinSet<()> = JoinSet::new();
    let mut caps: Vec<Arc<AtomicUsize>> = groups
        .iter()
        .map(|_| Arc::new(AtomicUsize::new(0)))
        .collect();
    let mut welcome_groups = 0usize;
    let mut welcomes_created = 0usize;
    // Schedule telemetry, reported at the end: proof the paths actually ran.
    let (mut n_kills, mut n_stalls, mut n_blips, mut n_suspends, mut n_relinks, mut n_races) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);

    // Lease `floors` as a new link of `chain`, on `consumer`'s transport.
    macro_rules! add_link {
        ($chain:expr, $floors:expr, $consumer:expr) => {{
            let consumer: usize = $consumer;
            let floors: HashMap<Topic, u64> = $floors;
            let subs: Vec<(Topic, u64)> = floors.iter().map(|(t, c)| (t.clone(), *c)).collect();
            let depth = if rng.random_range(0..3u8) == 0 {
                2
            } else {
                DEFAULT_LEASE_DEPTH
            };
            // A subscribe during a network fault can fail its cold open —
            // real consumers re-subscribe, so the fuzz does too. 40 tries
            // at 250ms is a 10s budget: comfortably past the longest
            // scheduled blip plus reconnect backoff, while still failing
            // fast on a genuinely wedged transport.
            let mut attempts = 0;
            let lease = loop {
                match transports[consumer].lease(subs.clone(), depth).await {
                    Ok(lease) => break lease,
                    Err(e) => {
                        attempts += 1;
                        assert!(attempts < 40, "lease kept failing: {e} (seed={seed})");
                        xmtp_common::time::sleep(Duration::from_millis(250)).await;
                    }
                }
            };
            let state = Arc::new(std::sync::Mutex::new(Collected::default()));
            let stall_ms = Arc::new(AtomicU64::new(0));
            let (die_tx, die_rx) = oneshot::channel();
            let index = links.len();
            spawn_collector(
                lease,
                state.clone(),
                stall_ms.clone(),
                die_rx,
                ends_tx.clone(),
                index,
                welcome_topics[consumer].clone(),
                latest.clone(),
            );
            links.push(Link {
                chain: $chain,
                consumer,
                floors,
                state,
                stall_ms,
                die: Some(die_tx),
                killed: false,
            });
        }};
    }

    // A transport-side drop (backpressure) recovers like a durable-cursor
    // re-subscribe: a new link from what the chain has received so far, on
    // the same consumer.
    macro_rules! relink {
        ($index:expr) => {{
            let index: usize = $index;
            if !links[index].killed {
                let chain = links[index].chain;
                let consumer = links[index].consumer;
                let mut floors = chains[&chain].clone();
                for link in links.iter().filter(|l| l.chain == chain) {
                    let state = link.state.lock().expect("checker lock");
                    for (topic, ids) in &state.ids {
                        if let (Some(floor), Some(max)) = (floors.get_mut(topic), ids.iter().max())
                        {
                            *floor = (*floor).max(*max);
                        }
                    }
                }
                add_link!(chain, floors, consumer);
                n_relinks += 1;
            }
        }};
    }

    for _ in 0..rounds {
        while let Ok(index) = ends_rx.try_recv() {
            relink!(index);
        }
        while let Some(res) = bursts.try_join_next() {
            res.expect("publish burst panicked");
        }
        match rng.random_range(0..20u8) {
            // Racing publish bursts: 1-3 member producers into one group,
            // concurrent with each other and with everything below.
            // Mid-run (welcome-op) groups have no producer members, so
            // alix carries those — the long-lived groups get the races.
            0..=5 => {
                let g = rng.random_range(0..groups.len());
                if g < initial_groups {
                    let k = rng.random_range(1..=3usize.min(n_producers));
                    let mut picks: Vec<usize> = (0..n_producers).collect();
                    let _ = picks.partial_shuffle(&mut rng, k);
                    for p in picks.into_iter().take(k) {
                        let count = rng.random_range(1..6usize);
                        let group = producer_groups[p][g].clone();
                        let cap = caps[g].clone();
                        n_races += 1;
                        bursts.spawn(async move {
                            for i in 0..count {
                                if cap.fetch_add(1, Ordering::Relaxed) >= PUBLISH_CAP {
                                    break;
                                }
                                publish_message(&group, format!("burst {i}").as_bytes(), seed)
                                    .await;
                            }
                        });
                    }
                } else {
                    let count = rng.random_range(1..6usize);
                    let group = groups[g].0.clone();
                    let cap = caps[g].clone();
                    bursts.spawn(async move {
                        for i in 0..count {
                            if cap.fetch_add(1, Ordering::Relaxed) >= PUBLISH_CAP {
                                break;
                            }
                            publish_message(&group, format!("burst {i}").as_bytes(), seed).await;
                        }
                    });
                }
            }
            // A new lease (its own chain) on a random consumer: 1-3
            // distinct random topics at random floors — shared topics at
            // lower floors are the yank engine — sometimes including that
            // consumer's welcome topic.
            6..=10 => {
                let consumer = rng.random_range(0..n_consumers);
                let mut floors = HashMap::new();
                let mut picks: Vec<usize> = (0..groups.len()).collect();
                let n = rng.random_range(1..4usize).min(groups.len());
                let _ = picks.partial_shuffle(&mut rng, n);
                for g in picks.into_iter().take(n) {
                    let topic = groups[g].1.clone();
                    let floor = random_floor(&mut rng, latest.load(Ordering::Relaxed));
                    floors.insert(topic, floor);
                }
                if rng.random_range(0..3u8) == 0 {
                    // Welcome ids are their own sequence: fuzz the two
                    // behavior classes (full replay vs owed-nothing).
                    let floor = if rng.random_range(0..4u8) == 0 {
                        u64::MAX / 2
                    } else {
                        0
                    };
                    floors.insert(welcome_topics[consumer].clone(), floor);
                }
                let chain = chains.len();
                chains.insert(chain, floors.clone());
                add_link!(chain, floors, consumer);
            }
            // Deliberate unsubscribe: no recovery obligation.
            11..=12 => {
                let alive: Vec<usize> = links
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| {
                        l.die.is_some() && !l.state.lock().expect("checker lock").ended
                    })
                    .map(|(i, _)| i)
                    .collect();
                if !alive.is_empty() {
                    let index = alive[rng.random_range(0..alive.len())];
                    links[index].killed = true;
                    drop(links[index].die.take());
                    n_kills += 1;
                }
            }
            // Stall a consumer — with a shallow channel this provokes the
            // transport's backpressure drop and the recovery chain.
            13..=14 => {
                if !links.is_empty() {
                    let index = rng.random_range(0..links.len());
                    links[index]
                        .stall_ms
                        .store(rng.random_range(400..1500), Ordering::Relaxed);
                    n_stalls += 1;
                }
            }
            // Selected consumer handles refer to the same backend proxy.
            // Every cut is a shared outage, including repeated disable calls.
            // All transports must recover their original delivery obligations.
            15 => {
                let blip = if rng.random_range(0..4u8) == 0 {
                    Duration::from_millis(rng.random_range(1200..2400))
                } else {
                    Duration::from_millis(rng.random_range(150..600))
                };
                let targets: Vec<usize> = if rng.random_range(0..4u8) == 0 {
                    (0..n_consumers).collect()
                } else {
                    vec![rng.random_range(0..n_consumers)]
                };
                for &c in &targets {
                    consumers[c]
                        .for_each_proxy(async |p| {
                            p.disable().await.expect("disable proxy");
                        })
                        .await;
                }
                xmtp_common::time::sleep(blip).await;
                for &c in &targets {
                    consumers[c]
                        .for_each_proxy(async |p| {
                            p.enable().await.expect("enable proxy");
                        })
                        .await;
                }
                n_blips += 1;
            }
            // The app-lifecycle pair, per consumer.
            16..=17 => {
                let c = rng.random_range(0..n_consumers);
                if suspended[c] {
                    let transport = transports[c].clone();
                    resume_handles.push(tokio::spawn(async move {
                        transport.resume().await.expect("resume failed");
                    }));
                    suspended[c] = false;
                } else {
                    transports[c].suspend().await.expect("suspend failed");
                    suspended[c] = true;
                    n_suspends += 1;
                }
            }
            // Real welcomes for 1-2 random consumers: a new group created
            // with them. The group joins the fuzz set too — a zero-history
            // topic born after the wires opened.
            18 if welcome_groups < 12 => {
                let k = rng.random_range(1..=2usize.min(n_consumers));
                let mut picks: Vec<usize> = (0..n_consumers).collect();
                let _ = picks.partial_shuffle(&mut rng, k);
                let members: Vec<_> = picks[..k]
                    .iter()
                    .map(|&c| consumers[c].inbox_id())
                    .collect();
                let group = alix
                    .create_group_with_members(&members, None, None)
                    .await
                    .expect("create_group_with_members failed");
                let topic = Topic::new_group_message(group.group_id);
                groups.push((group, topic));
                caps.push(Arc::new(AtomicUsize::new(0)));
                welcome_groups += 1;
                welcomes_created += k;
            }
            // Let the wire breathe.
            _ => xmtp_common::time::sleep(Duration::from_millis(rng.random_range(10..80))).await,
        }
        xmtp_common::time::sleep(Duration::from_millis(rng.random_range(5..30))).await;
    }

    // Wind down: back on the network, every racing burst landed, recover
    // any still-pending drops, and wait out every resume() (each resolves
    // at a caught-up live wire).
    for (c, transport) in transports.iter().enumerate() {
        if suspended[c] {
            let transport = transport.clone();
            resume_handles.push(tokio::spawn(async move {
                transport.resume().await.expect("resume failed");
            }));
        }
    }
    while let Some(res) = bursts.join_next().await {
        res.expect("publish burst panicked");
    }
    for handle in resume_handles {
        tokio::time::timeout(SETTLE, handle)
            .await
            .unwrap_or_else(|_| panic!("resume() never resolved (seed={seed})"))
            .expect("resume task panicked");
    }
    while let Ok(index) = ends_rx.try_recv() {
        relink!(index);
    }

    // Sentinels bound the run; ground truth is the server's own record.
    // The welcome sentinel is one more group created with EVERY consumer,
    // so each welcome topic gets a live edge to confirm.
    for (group, _) in &groups {
        publish_message(group, b"sentinel", seed).await;
    }
    {
        let members: Vec<_> = consumers.iter().map(|c| c.inbox_id()).collect();
        alix.create_group_with_members(&members, None, None)
            .await
            .expect("welcome sentinel failed");
    }
    // Computed here, not at setup: the fuzz set grows with the groups the
    // welcome op created mid-run.
    let group_keys: Vec<_> = groups
        .iter()
        .map(|(group, topic)| (group.group_id, topic.clone()))
        .collect();
    let mut truth = ground_truth(api, &group_keys).await;
    for (c, consumer) in consumers.iter().enumerate() {
        let welcome_truth: BTreeSet<u64> = api
            .query_welcome_messages(consumer.context.installation_id())
            .await
            .expect("welcome ground-truth query failed")
            .iter()
            .map(|w| w.cursor.0)
            .collect();
        truth.insert(welcome_topics[c].clone(), welcome_truth);
    }
    let max_id = |topic: &Topic| truth.get(topic).and_then(|ids| ids.last()).copied();

    // Every surviving lease must reach its catch-up marker AND the live
    // edge of each topic it asked for. Hitting the deadline is the loss
    // (or hang) detector firing.
    let deadline = tokio::time::Instant::now() + SETTLE;
    loop {
        // A drop landing during the wind-down (residual stall, sentinel
        // burst) still recovers here — otherwise its whole chain would end
        // unchecked and a loss could pass silently.
        while let Ok(index) = ends_rx.try_recv() {
            relink!(index);
        }
        let lagging: Vec<String> = links
            .iter()
            .enumerate()
            .filter(|(_, link)| !link.killed && !link.state.lock().expect("checker lock").ended)
            .flat_map(|(i, link)| {
                let state = link.state.lock().expect("checker lock");
                let mut lags = Vec::new();
                if state.catch_ups == 0 {
                    lags.push(format!("link {i}: no CatchUpComplete"));
                }
                for (topic, floor) in &link.floors {
                    let Some(max) = max_id(topic) else { continue };
                    if max <= *floor {
                        continue; // subscribed past the sentinel — owed nothing
                    }
                    let got = state.ids.get(topic).is_some_and(|ids| ids.contains(&max));
                    if !got {
                        lags.push(format!("link {i}: missing sentinel {max} on {topic}"));
                    }
                }
                lags
            })
            .collect();
        if lagging.is_empty() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "leases never converged (seed={seed}): {lagging:?}"
        );
        xmtp_common::time::sleep(Duration::from_millis(250)).await;
    }

    let n_drops = links
        .iter()
        .filter(|l| !l.killed && l.state.lock().expect("checker lock").ended)
        .count();
    tracing::info!(
        "bidi transport fuzz schedule: seed={seed} producers={n_producers} \
         consumers={n_consumers} links={} drops={n_drops} relinks={n_relinks} \
         kills={n_kills} stalls={n_stalls} blips={n_blips} suspends={n_suspends} \
         races={n_races} welcomes={welcomes_created}",
        links.len(),
    );

    // Per-link: strictly increasing above its own floor — one sweep proves
    // delivery order, exactly-once, and that nothing at-or-below the floor
    // leaked through (both demux lanes honor the holder's floor; only
    // cursor-less frames fail open, and those never reach a collector).
    for (i, link) in links.iter().enumerate() {
        let state = link.state.lock().expect("checker lock");
        assert!(
            state.catch_ups <= 1,
            "link {i} caught up {} times (seed={seed})",
            state.catch_ups
        );
        for topic in state.ids.keys() {
            assert!(
                link.floors.contains_key(topic),
                "link {i} received {topic}, which it never leased (seed={seed})"
            );
        }
        for (topic, floor) in &link.floors {
            let ids: &[u64] = state.ids.get(topic).map(|v| &v[..]).unwrap_or(&[]);
            let mut last = *floor;
            for id in ids {
                assert!(
                    *id > last,
                    "link {i} delivery on {topic} not strictly increasing above \
                     the floor: {id} after {last} (floor {floor}, seed={seed})"
                );
                last = *id;
            }
            // A link alive at the end holds the complete suffix.
            if !state.ended
                && let Some(truth_ids) = truth.get(topic)
            {
                let got: BTreeSet<u64> = ids.iter().copied().collect();
                let missing: Vec<u64> = truth_ids
                    .iter()
                    .filter(|id| **id > *floor && !got.contains(*id))
                    .copied()
                    .collect();
                assert!(
                    missing.is_empty(),
                    "link {i} lost {missing:?} on {topic} above floor {floor} (seed={seed})"
                );
            }
        }
    }

    // Per-chain: the union across recovery links covers everything above
    // the ORIGINAL floors — the durable-cursor recovery contract. Only a
    // chain the schedule deliberately unsubscribed is exempt; a chain whose
    // last link ended AFTER convergence already holds the sentinel, so
    // `ended` there does not weaken the union.
    for (chain, floors) in &chains {
        let deliberately_ended = links
            .iter()
            .rfind(|l| l.chain == *chain)
            .is_some_and(|l| l.killed);
        if deliberately_ended {
            continue;
        }
        let mut union: HashMap<Topic, BTreeSet<u64>> = HashMap::new();
        for link in links.iter().filter(|l| l.chain == *chain) {
            let state = link.state.lock().expect("checker lock");
            for (topic, ids) in &state.ids {
                union.entry(topic.clone()).or_default().extend(ids);
            }
        }
        for (topic, floor) in floors {
            let Some(truth_ids) = truth.get(topic) else {
                continue;
            };
            let got = union.get(topic).cloned().unwrap_or_default();
            let missing: Vec<u64> = truth_ids
                .iter()
                .filter(|id| **id > *floor && !got.contains(*id))
                .copied()
                .collect();
            assert!(
                missing.is_empty(),
                "chain {chain} lost {missing:?} on {topic} above floor {floor} (seed={seed})"
            );
        }
    }
}
