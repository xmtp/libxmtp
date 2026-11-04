//! Bidirectional stream fuzz tests: server contract.

use super::*;

fn mutate(adds: Vec<(Topic, u64)>, id: u64) -> Update {
    BackendBinding::build_mutate(adds, [], id)
}

fn remove_mutate(topic: Topic, id: u64) -> Update {
    BackendBinding::build_mutate([], [topic], id)
}

/// Mirror updates and validate the server's ordered response stream.
struct ContractState {
    seed: u64,
    unacked: HashMap<u64, Update>,
    acked: HashSet<u64>,
    active: HashMap<Topic, u64>,
    targets: HashMap<Topic, u64>,
    last: HashMap<Topic, u64>,
    received: HashMap<Topic, BTreeSet<u64>>,
    pending_remove: HashMap<u64, Topic>,
}

impl ContractState {
    fn new(seed: u64) -> Self {
        Self {
            seed,
            unacked: HashMap::new(),
            acked: HashSet::new(),
            active: HashMap::new(),
            targets: HashMap::new(),
            last: HashMap::new(),
            received: HashMap::new(),
            pending_remove: HashMap::new(),
        }
    }

    fn sent(&mut self, update: Update) {
        assert!(self.unacked.insert(update.id, update).is_none());
    }

    fn sent_remove(&mut self, id: u64, topic: &Topic) {
        self.pending_remove.insert(id, topic.clone());
        self.sent(remove_mutate(topic.clone(), id));
    }

    fn position_bound(&self, topic: &Topic) -> u64 {
        self.last.get(topic).copied().unwrap_or(0)
    }

    fn observe(&mut self, event: BidiEvent) {
        let seed = self.seed;
        match event {
            BidiEvent::Started { .. } => {}
            BidiEvent::Applied { id, targets } => {
                assert!(
                    self.acked.insert(id),
                    "update {id} acked twice (seed={seed})"
                );
                let update = self.unacked.remove(&id).expect("ack for an unsent update");
                for wire_topic in update.removes {
                    let topic = Topic::parse(&wire_topic.topic).expect("remove topic");
                    self.active.remove(&topic);
                    self.last.remove(&topic);
                    self.targets.remove(&topic);
                    self.received.remove(&topic);
                }
                self.pending_remove.remove(&id);
                let mut added = HashSet::new();
                for subscription in update.adds {
                    let topic =
                        Topic::parse(&subscription.topic.expect("subscription topic").topic)
                            .expect("add topic");
                    let floor = subscription.cursor.unwrap_or_default().sequence_id;
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        self.active.entry(topic.clone())
                    {
                        entry.insert(floor);
                        self.last.insert(topic.clone(), floor);
                        added.insert(topic);
                    }
                }
                let mut seen = HashSet::new();
                for (topic, target) in targets {
                    assert!(
                        added.contains(&topic),
                        "target for an ineffective add (seed={seed})"
                    );
                    assert!(seen.insert(topic.clone()), "duplicate target (seed={seed})");
                    if target > self.active[&topic] {
                        self.targets.insert(topic, target);
                    }
                }
            }
            BidiEvent::GroupMessages { messages } | BidiEvent::WelcomeMessages { messages } => {
                for message in &messages {
                    let (topic, cursor) = gm_parts(message).expect("valid envelope metadata");
                    let last = self
                        .last
                        .get_mut(&topic)
                        .expect("delivery on an inactive topic");
                    assert!(
                        cursor > *last,
                        "cursor {cursor} follows {last} on {topic} (seed={seed})"
                    );
                    *last = cursor;
                    self.received
                        .entry(topic.clone())
                        .or_default()
                        .insert(cursor);
                    if self
                        .targets
                        .get(&topic)
                        .is_some_and(|target| cursor >= *target)
                    {
                        self.targets.remove(&topic);
                    }
                }
            }
        }
    }
}

/// One raw connection under fuzz, with its own checker and update counter —
/// the server scopes `Update.id` values per stream, so each connection mints its
/// own and audits its own contract.
struct FuzzConn<C> {
    conn: C,
    state: ContractState,
    next_update_id: u64,
}

/// Backend server-contract fuzz: random subscription churn on up to ten raw
/// connections while up to ten member clients race publishes into the same
/// groups, with every frame checked against the wire guarantees the client
/// ledger is built on — independently per connection.
#[xmtp_common::timeout(Duration::from_secs(300))]
#[xmtp_common::test(unwrap_try = true)]
async fn fuzz_server_honors_update_acknowledgements_and_targets() {
    let seed = fuzz_seed();
    let rounds = fuzz_rounds(60);
    let mut rng = StdRng::seed_from_u64(seed);

    tester!(alix);
    let n_producers = rng.random_range(2..=10usize);
    let n_conns = rng.random_range(2..=10usize);
    tracing::info!(
        "bidi server-contract fuzz: seed={seed} rounds={rounds} \
         producers={n_producers} conns={n_conns}"
    );
    let mut producers = Vec::new();
    for i in 0..n_producers {
        producers.push(
            TesterBuilder::new()
                .with_name(&format!("update_prod_{i}"))
                .build()
                .await,
        );
    }
    let member_ids: Vec<_> = producers.iter().map(|p| p.inbox_id()).collect();
    let mut groups = Vec::new();
    for _ in 0..3 {
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
    // Every producer joins every group so racing bursts are real member
    // sends, not creator-only traffic. Each member's FIRST send piggybacks
    // its one-time post-join KeyUpdate commit (PCS rotation) — flush those
    // serially here, so the racing bursts later contend only as app
    // messages at a stable epoch. Join-time epoch churn is MLS-layer
    // behavior with its own tests; this fuzz targets wire delivery.
    let group_ids: Vec<_> = groups.iter().map(|(group, _)| group.group_id).collect();
    let mut producer_groups = Vec::new();
    for producer in &producers {
        let handles = joined_groups(producer, &group_ids).await;
        for handle in &handles {
            publish_message(handle, b"settle", seed).await;
        }
        producer_groups.push(handles);
    }
    let group_keys: Vec<_> = groups
        .iter()
        .map(|(group, topic)| (group.group_id, topic.clone()))
        .collect();

    // The initial adds use REAL cursors (queried), not just zero, so the
    // nonzero-floor boundary — everything strictly above F — is exercised
    // against the server from the first update. One pre-open snapshot serves
    // every connection: positions only grow, so `floor < latest_at_query`
    // proves "below the server-side position" at any later open too.
    let api = alix.context.api();
    let mut known_floor: HashMap<Topic, u64> = HashMap::new();
    for (group, topic) in &groups {
        let latest = api
            .query_latest_group_message(group.group_id)
            .await
            .expect("latest query failed")
            .map(|m| m.cursor.0)
            .unwrap_or(0);
        known_floor.insert(topic.clone(), latest);
    }
    // All connections ride alix's channel: h2 multiplexes them into
    // independent server-side subscriptions, which is the surface under
    // test — per-identity channels would only slow the schedule down.
    let mut conns = Vec::new();
    for _ in 0..n_conns {
        let mut initial: Vec<(Topic, u64)> = Vec::new();
        for (_, topic) in &groups {
            let latest = known_floor[topic];
            let floor = match rng.random_range(0..3u8) {
                0 => 0,
                1 => latest / 2,
                _ => latest.saturating_sub(2),
            };
            initial.push((topic.clone(), floor));
        }
        let conn = BidiConnection::open(&api.api_client, mutate(initial.clone(), 1))
            .await
            .expect("open failed");
        let mut state = ContractState::new(seed);
        state.sent(mutate(initial.clone(), 1));
        conns.push(FuzzConn {
            conn,
            state,
            next_update_id: 2,
        });
    }
    let mut removes_sent = 0usize;
    let mut n_races = 0usize;

    // Racing publish bursts: each task is one member client's burst into
    // one group, running concurrently with every other task and with the
    // subscription churn below.
    let mut bursts: JoinSet<()> = JoinSet::new();
    let caps: Vec<Arc<AtomicUsize>> = groups
        .iter()
        .map(|_| Arc::new(AtomicUsize::new(0)))
        .collect();
    for _ in 0..rounds {
        match rng.random_range(0..10u8) {
            // Spawn racing bursts into a random group from 1-3 producers.
            0..=3 => {
                let g = rng.random_range(0..groups.len());
                let k = rng.random_range(1..=3usize.min(n_producers));
                let mut picks: Vec<usize> = (0..n_producers).collect();
                let _ = picks.partial_shuffle(&mut rng, k);
                for p in picks.into_iter().take(k) {
                    let count = rng.random_range(1..5usize);
                    let group = producer_groups[p][g].clone();
                    let cap = caps[g].clone();
                    n_races += 1;
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
            // A cursored (re-)add on a random connection — lower re-adds
            // yank topics between updates. Floors are biased around the
            // topic's OWN observed position, so yanks (and their
            // live-holds) actually arm; occasionally two topics ride one
            // update.
            4..=6 => {
                let c = rng.random_range(0..conns.len());
                let mut picks: Vec<usize> = (0..groups.len()).collect();
                let _ = picks.partial_shuffle(&mut rng, 2);
                let n = if rng.random_range(0..3u8) == 0 { 2 } else { 1 };
                let fc = &mut conns[c];
                let update = fc.next_update_id;
                fc.next_update_id += 1;
                let mut adds = Vec::new();
                for g in picks.into_iter().take(n) {
                    let topic = &groups[g].1;
                    let bound = fc.state.position_bound(topic);
                    let floor = match rng.random_range(0..5u8) {
                        0 => 0,
                        1 => bound / 2,
                        2 => bound.saturating_sub(3),
                        3 => bound,
                        _ => bound + 10_000,
                    };
                    adds.push((topic.clone(), floor));
                }
                fc.conn
                    .mutate(mutate(adds.clone(), update))
                    .await
                    .expect("mutate failed");
                fc.state.sent(mutate(adds, update));
            }
            // A removes-only Mutate on a random connection: always acked
            // (echoing the minted id), and that connection owes the topic
            // silence from the ack until a re-add.
            7 if removes_sent < 3 => {
                let c = rng.random_range(0..conns.len());
                let (_, topic) = &groups[rng.random_range(0..groups.len())];
                let fc = &mut conns[c];
                if !fc.state.pending_remove.values().any(|t| t == topic) {
                    let update = fc.next_update_id;
                    fc.next_update_id += 1;
                    fc.conn
                        .mutate(remove_mutate(topic.clone(), update))
                        .await
                        .expect("remove mutate failed");
                    fc.state.sent_remove(update, topic);
                    removes_sent += 1;
                }
            }
            // Drain whatever the server has ready, on every connection —
            // and reap any finished publish bursts.
            _ => {
                while let Some(res) = bursts.try_join_next() {
                    res.expect("publish burst panicked");
                }
                for fc in conns.iter_mut() {
                    loop {
                        match tokio::time::timeout(Duration::from_millis(20), fc.conn.next()).await
                        {
                            Ok(Some(event)) => fc.state.observe(event),
                            Ok(None) => panic!("connection died mid-run (seed={seed})"),
                            Err(_) => break,
                        }
                    }
                }
            }
        }
    }

    // Every racing burst lands before the run is bounded.
    while let Some(res) = bursts.join_next().await {
        res.expect("publish burst panicked");
    }

    // Always-ack: every update every connection sent must resolve.
    for fc in conns.iter_mut() {
        let deadline = tokio::time::Instant::now() + SETTLE;
        while !fc.state.unacked.is_empty() || !fc.state.targets.is_empty() {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let pending: Vec<u64> = fc.state.unacked.keys().copied().collect();
            assert!(
                !remaining.is_zero(),
                "updates never acked: {pending:?} (seed={seed})"
            );
            match tokio::time::timeout(remaining, fc.conn.next()).await {
                Ok(Some(event)) => fc.state.observe(event),
                Ok(None) => panic!("connection died awaiting acks (seed={seed})"),
                Err(_) => panic!("updates never acked: {pending:?} (seed={seed})"),
            }
        }
    }

    // Sentinels mark the live edge; with every update acked they arrive on
    // the live lane, and receiving them bounds each connection's
    // completeness check.
    for (group, _) in &groups {
        publish_message(group, b"sentinel", seed).await;
    }
    let truth = ground_truth(api, &group_keys).await;
    for fc in conns.iter_mut() {
        let deadline = tokio::time::Instant::now() + SETTLE;
        loop {
            let done = groups.iter().all(|(_, topic)| {
                let Some(floor) = fc.state.active.get(topic) else {
                    return true;
                };
                let Some(max) = truth.get(topic).and_then(|ids| ids.last()) else {
                    return true;
                };
                if max <= floor {
                    return true;
                }
                fc.state
                    .received
                    .get(topic)
                    .is_some_and(|ids| ids.contains(max))
            });
            if done {
                break;
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(
                !remaining.is_zero(),
                "sentinels never arrived (seed={seed})"
            );
            match tokio::time::timeout(remaining, fc.conn.next()).await {
                Ok(Some(event)) => fc.state.observe(event),
                Ok(None) => panic!("connection died awaiting sentinels (seed={seed})"),
                Err(_) => panic!("sentinels never arrived (seed={seed})"),
            }
        }
    }

    // No loss, per connection: everything above the lowest cursor it ever
    // asked for was served — by some update or the live lane. Removed topics
    // are exempt: their delivery obligation ended mid-run.
    for (c, fc) in conns.iter().enumerate() {
        for (topic, min) in &fc.state.active {
            let Some(truth_ids) = truth.get(topic) else {
                continue;
            };
            let got = fc.state.received.get(topic).cloned().unwrap_or_default();
            let missing: Vec<u64> = truth_ids
                .iter()
                .filter(|id| **id > *min && !got.contains(*id))
                .copied()
                .collect();
            assert!(
                missing.is_empty(),
                "server never served {missing:?} on {topic} above cursor {min} \
                 to connection {c} (seed={seed})"
            );
        }
    }
    let updates: u64 = conns.iter().map(|fc| fc.next_update_id - 1).sum();
    tracing::info!(
        "bidi server-contract fuzz done: seed={seed} conns={} producers={n_producers} \
         races={n_races} updates={updates} removes={removes_sent}",
        conns.len(),
    );
}
