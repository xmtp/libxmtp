//! The supervisor owns process lifetimes, rounds, and verdicts.
use crate::{
    RunArgs,
    check::{InstanceSnapshot, Oracle, RollCall},
    evidence::{DatabaseCopy, StoppedWriters},
    faults::network::{NetworkController, NetworkFault},
    ledger::{Limits, RunLedger},
    population::{self, INBOXES, PROXY_SLOTS, SHARED_SLOT},
    process::{ChildLogs, InstanceProcess},
    protocol::{Command, InstanceConfig, Operation},
    schedule::{FaultWindow, GroupView, InstanceView, Scheduler},
};
use anyhow::{Context, Result, bail, ensure};
use futures::future::join_all;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::sync::{Barrier, RwLock};
use xmtp_common::time::{Duration, Instant, sleep, timeout};
use xmtp_mls::diagnostics::{CheckpointSnapshot, RetryBudgets};

const CHAOS_SECONDS: u64 = 60;
const ROLL_POLL_MS: u64 = 100;

struct Supervisor {
    network: NetworkController,
    configs: Mutex<Vec<InstanceConfig>>,
    processes: RwLock<BTreeMap<usize, Arc<InstanceProcess>>>,
    ledger: Mutex<RunLedger>,
    logs: ChildLogs,
    registration: tokio::sync::Mutex<()>,
}

impl Supervisor {
    async fn process(&self, slot: usize) -> Result<Arc<InstanceProcess>> {
        self.processes
            .read()
            .await
            .get(&slot)
            .cloned()
            .context("instance is not running")
    }
    async fn start(&self, slot: usize) -> Result<()> {
        let config = self.configs.lock().expect("configs mutex")[slot].clone();
        let config_path = {
            let ledger = self.ledger.lock().expect("ledger mutex");
            let name = format!("instance-{slot}.json");
            ledger.write_json(&name, &serde_json::to_value(&config)?)?;
            ledger.root().join(name)
        };
        let process = InstanceProcess::spawn(&config, &config_path, self.logs.clone()).await?;
        self.processes.write().await.insert(slot, Arc::new(process));
        self.save_population().await
    }
    async fn save_population(&self) -> Result<()> {
        let processes = self.processes.read().await;
        let configs = self.configs.lock().expect("configs mutex");
        let population=processes.values().map(|p|json!({"instance":p.slot,"inbox_id":p.inbox_id,"installation_id":p.installation_id,"config":configs[p.slot]})).collect::<Vec<_>>();
        self.ledger
            .lock()
            .expect("ledger mutex")
            .write_json("population.json", &json!(population))
    }
    fn record(&self, kind: &str, value: Value) -> Result<()> {
        self.ledger
            .lock()
            .expect("ledger mutex")
            .append(kind, &value)
    }
    async fn operation(&self, slot: usize, operation: Operation) -> Result<Value> {
        if let Operation::NewInstallation { inbox_index } = operation {
            let _registration = self.registration.lock().await;
            let active = self
                .processes
                .read()
                .await
                .keys()
                .copied()
                .collect::<BTreeSet<_>>();
            let next = {
                let configs = self.configs.lock().expect("configs mutex");
                configs
                    .iter()
                    .find(|c| {
                        c.inbox_index == inbox_index
                            && c.slot != SHARED_SLOT
                            && !active.contains(&c.slot)
                    })
                    .map(|c| c.slot)
            };
            if let Some(next) = next {
                self.start(next).await?;
                return Ok(json!({"new_instance":next}));
            }
            return Ok(json!({"cap_reached":true}));
        }
        self.process(slot)
            .await?
            .call(Command::Operation { operation })
            .await
    }
    async fn scheduled_operation(
        &self,
        round: u64,
        burst: usize,
        slot: usize,
        operation: Operation,
        deadline: Option<Instant>,
    ) -> Result<bool> {
        let started = Instant::now();
        let recorded_operation = serde_json::to_value(&operation)?;
        self.record("ops",json!({"event":"start","round":round,"burst":burst,"instance":slot,"operation":operation}))?;
        let outcome = if let Some(deadline) = deadline {
            timeout(
                deadline.saturating_duration_since(Instant::now()),
                self.operation(slot, operation),
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!("chaos deadline reached; operation outcome remains uncertain")
            })
            .and_then(|result| result)
        } else {
            self.operation(slot, operation).await
        };
        let success = outcome.is_ok();
        let value = match outcome {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":format!("{error:#}")}),
        };
        self.record("ops",json!({"event":"end","round":round,"burst":burst,"instance":slot,"operation":recorded_operation,"duration_ms":started.elapsed().as_millis(),"outcome":value}))?;
        Ok(success)
    }
    async fn burst(
        &self,
        round: u64,
        index: usize,
        burst: &crate::schedule::Burst,
        deadline: Instant,
    ) -> Result<(usize, usize)> {
        let mut by_slot: BTreeMap<usize, Vec<Operation>> = BTreeMap::new();
        for op in &burst.operations {
            by_slot
                .entry(op.instance)
                .or_default()
                .push(op.operation.clone());
        }
        let barrier = Arc::new(Barrier::new(by_slot.len()));
        let results = join_all(by_slot.into_iter().map(|(slot, operations)| {
            let barrier = barrier.clone();
            async move {
                barrier.wait().await;
                let mut ok = 0;
                let mut errors = 0;
                for op in operations {
                    if self
                        .scheduled_operation(round, index, slot, op, Some(deadline))
                        .await?
                    {
                        ok += 1;
                    } else {
                        errors += 1;
                    }
                }
                Ok::<_, anyhow::Error>((ok, errors))
            }
        }))
        .await;
        let mut counts = (0, 0);
        for result in results {
            let (ok, err) = result?;
            counts.0 += ok;
            counts.1 += err;
        }
        Ok(counts)
    }
    async fn fault(&self, round: u64, window: &FaultWindow, origin: Instant) -> Result<()> {
        let due = Duration::from_millis(window.start_ms);
        if let Some(delay) = due.checked_sub(origin.elapsed()) {
            sleep(delay).await;
        }
        self.record("faults",json!({"event":"start","round":round,"fault":window,"elapsed_ms":origin.elapsed().as_millis()}))?;
        let network = serde_json::from_value::<NetworkFault>(json!(window.kind)).ok();
        if let Some(fault) = network {
            self.network.apply(window.instance, fault).await?;
        } else if let Some(kind) = window.kind.strip_prefix("disk_") {
            self.process(window.instance)
                .await?
                .call(Command::Disk {
                    kind: kind.into(),
                    duration_ms: window.duration_ms,
                })
                .await?;
        } else if window.kind == "connection_loss" {
            self.process(window.instance)
                .await?
                .call(Command::Disconnect {
                    duration_ms: window.duration_ms,
                })
                .await?;
        } else if window.kind == "process_kill" {
            self.process(window.instance).await?.crash().await?;
        } else {
            bail!("unknown fault {}", window.kind);
        }
        sleep(Duration::from_millis(window.duration_ms)).await;
        if let Some(fault) = network {
            self.network.clear_fault(window.instance, fault).await?;
        } else if window.kind == "process_kill" {
            self.start(window.instance).await?;
        } else {
            self.process(window.instance)
                .await?
                .call(Command::ClearFaults)
                .await?;
        }
        self.record("faults",json!({"event":"end","round":round,"fault":window,"elapsed_ms":origin.elapsed().as_millis()}))?;
        Ok(())
    }
    async fn clear(&self) -> Result<()> {
        self.network.clear_all().await?;
        let processes = self
            .processes
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for p in processes {
            if p.call(Command::ClearFaults).await.is_err() {
                p.stop().await?;
                self.start(p.slot).await?;
            }
        }
        Ok(())
    }
    async fn checkpoint(&self) -> Result<Vec<InstanceSnapshot>> {
        use xmtp_mls::diagnostics::{BarrierCause, BarrierFailure, BarrierTopicSnapshot};
        use xmtp_proto::types::Topic;

        let processes = self
            .processes
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let started = Instant::now();
        let budget = Duration::from_millis(RetryBudgets::default().barrier_ms);
        let mut previous = None;
        loop {
            let barriers = join_all(processes.iter().map(|process| async move {
                Ok::<CheckpointSnapshot, anyhow::Error>(serde_json::from_value(
                    process.call(Command::Checkpoint).await?,
                )?)
            }));
            // The first pass uses the SDK's own deadline. Later passes share its budget.
            let barriers = if let Some(previous) = previous.take() {
                match timeout(budget.saturating_sub(started.elapsed()), barriers).await {
                    Ok(barriers) => barriers,
                    Err(_) => return Ok(previous),
                }
            } else {
                barriers.await
            };
            let barriers = barriers.into_iter().collect::<Result<Vec<_>>>()?;
            // No installation is sampled while another barrier is still driving processing.
            let states = join_all(processes.iter().zip(barriers).map(
                |(process, checkpoint)| async move {
                    let state = process.call(Command::Snapshot).await?;
                    let stream_owner =
                        self.configs.lock().expect("configs mutex")[process.slot].stream_owner;
                    Ok::<_, anyhow::Error>(InstanceSnapshot {
                        instance: process.slot,
                        inbox_id: process.inbox_id.clone(),
                        installation_id: process.installation_id.clone(),
                        stream_owner,
                        groups: serde_json::from_value(state["groups"].clone())?,
                        checkpoint,
                    })
                },
            ))
            .await;
            let mut states = states.into_iter().collect::<Result<Vec<_>>>()?;
            if states.iter().any(|state| {
                state.checkpoint.failure.is_some()
                    || state
                        .checkpoint
                        .topics
                        .iter()
                        .any(|topic| topic.cause.is_some())
            }) {
                return Ok(states);
            }

            let mut common = BTreeMap::<String, u64>::new();
            for state in &states {
                for group in state.groups.iter().filter(|group| group.active) {
                    let topic =
                        hex::encode(Topic::new_group_message(hex::decode(&group.group_id)?));
                    let maximum = common.entry(topic.clone()).or_default();
                    *maximum = (*maximum).max(group.cursor);
                    if let Some(barrier) = state
                        .checkpoint
                        .topics
                        .iter()
                        .find(|entry| entry.topic == topic)
                    {
                        *maximum = (*maximum)
                            .max(barrier.processed)
                            .max(barrier.target.unwrap_or_default());
                    }
                }
            }
            let mut drifted = false;
            for state in &mut states {
                for group in state.groups.iter().filter(|group| group.active) {
                    let topic =
                        hex::encode(Topic::new_group_message(hex::decode(&group.group_id)?));
                    let target = common[&topic];
                    if let Some(barrier) = state
                        .checkpoint
                        .topics
                        .iter_mut()
                        .find(|entry| entry.topic == topic)
                    {
                        if barrier.target == Some(target)
                            && barrier.processed == target
                            && group.cursor == target
                        {
                            continue;
                        }
                        barrier.target = Some(target);
                        barrier.cause = Some(BarrierCause::ProcessingPending);
                    } else {
                        state.checkpoint.topics.push(BarrierTopicSnapshot {
                            topic,
                            target: Some(target),
                            received: 0,
                            processed: group.cursor,
                            unresolved_welcomes: Vec::new(),
                            inactive: false,
                            cause: Some(BarrierCause::TargetPending),
                        });
                    }
                    state.checkpoint.failure = Some(BarrierFailure::Deadline);
                    drifted = true;
                }
            }
            if !drifted || started.elapsed() >= budget {
                return Ok(states);
            }
            previous = Some(states);
        }
    }
    async fn handover(&self) -> Result<()> {
        let (from, to) = {
            let c = self.configs.lock().expect("configs mutex");
            if c[0].stream_owner {
                (0, SHARED_SLOT)
            } else {
                (SHARED_SLOT, 0)
            }
        };
        self.process(from)
            .await?
            .call(Command::Stream { enabled: false })
            .await?;
        self.process(to)
            .await?
            .call(Command::Stream { enabled: true })
            .await?;
        {
            let mut c = self.configs.lock().expect("configs mutex");
            c[from].stream_owner = false;
            c[to].stream_owner = true;
        }
        self.save_population().await?;
        self.record(
            "ops",
            json!({"event":"stream_handover","from":from,"to":to}),
        )
    }
    async fn rollcall(
        &self,
        round: u64,
        seed: u64,
        snapshots: &[InstanceSnapshot],
    ) -> Result<Vec<RollCall>> {
        let processes = self
            .processes
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for result in join_all(
            processes
                .iter()
                .map(|p| p.call(Command::Tokens { tokens: Vec::new() })),
        )
        .await
        {
            result?;
        }
        let mut calls = Vec::new();
        let mut sends = Vec::new();
        let mut sent = BTreeSet::new();
        for instance in snapshots {
            if instance.checkpoint.failure.is_some() {
                continue;
            }
            for group in &instance.groups {
                if !group.active
                    || !sent.insert((group.group_id.clone(), instance.installation_id.clone()))
                {
                    continue;
                }
                let token = format!(
                    "xchaos:{seed}:{round}:{}:{}",
                    group.group_id, instance.installation_id
                );
                let expected_installations = group
                    .members
                    .iter()
                    .map(|m| m.installation_id.clone())
                    .collect::<BTreeSet<_>>();
                let expected_stream_installations = snapshots
                    .iter()
                    .filter(|i| {
                        i.stream_owner && expected_installations.contains(&i.installation_id)
                    })
                    .map(|i| i.installation_id.clone())
                    .collect();
                calls.push(RollCall {
                    group_id: group.group_id.clone(),
                    token: token.clone(),
                    sender_installation: instance.installation_id.clone(),
                    expected_installations,
                    expected_stream_installations,
                    sync_received: BTreeSet::new(),
                    stream_received: BTreeSet::new(),
                    elapsed_ms: 0,
                });
                sends.push(self.scheduled_operation(
                    round,
                    usize::MAX,
                    instance.instance,
                    Operation::Send {
                        group: group.group_id.clone(),
                        token,
                    },
                    None,
                ));
            }
        }
        let finished = join_all(sends.into_iter().map(|send| async {
            let result = send.await;
            (result, Instant::now())
        }))
        .await;
        let mut timers = Vec::new();
        for (call, (result, finished)) in calls.iter().zip(finished) {
            self.record(
                "ops",
                json!({"event":"rollcall","token":call.token,"sent":result?}),
            )?;
            timers.push(finished);
        }
        self.checkpoint().await?;
        let tokens = calls.iter().map(|c| c.token.clone()).collect::<Vec<_>>();
        let budget_ms = RetryBudgets::default().barrier_ms;
        loop {
            let received = join_all(processes.iter().map(|p| {
                p.call(Command::Tokens {
                    tokens: tokens.clone(),
                })
            }))
            .await;
            for (process, received) in processes.iter().zip(received) {
                let received = received?;
                let sync: Vec<String> = serde_json::from_value(received["sync"].clone())?;
                let stream: Vec<String> = serde_json::from_value(received["stream"].clone())?;
                let owner = self.configs.lock().expect("configs mutex")[process.slot].stream_owner;
                for call in &mut calls {
                    if sync.contains(&call.token) {
                        call.sync_received.insert(process.installation_id.clone());
                    }
                    if owner && stream.contains(&call.token) {
                        call.stream_received.insert(process.installation_id.clone());
                    }
                }
            }
            for (call, timer) in calls.iter_mut().zip(&timers) {
                call.elapsed_ms = timer.elapsed().as_millis() as u64;
            }
            if calls.iter().all(|call| {
                (call.expected_installations.is_subset(&call.sync_received)
                    && call
                        .expected_stream_installations
                        .is_subset(&call.stream_received))
                    || call.elapsed_ms > budget_ms
            }) {
                break;
            }
            for call in &calls {
                if call.expected_installations.is_subset(&call.sync_received)
                    || call.elapsed_ms > budget_ms
                {
                    continue;
                }
                let process = processes
                    .iter()
                    .find(|p| p.installation_id == call.sender_installation)
                    .context("roll-call sender missing")?;
                self.record("ops",json!({"event":"retry_publish","round":round,"instance":process.slot,"group":call.group_id}))?;
                let outcome = process
                    .call(Command::Publish {
                        group: call.group_id.clone(),
                    })
                    .await;
                self.record("ops",json!({"event":"publish_result","instance":process.slot,"group":call.group_id,"error":outcome.err().map(|error|format!("{error:#}"))}))?;
            }
            self.checkpoint().await?;
            sleep(Duration::from_millis(ROLL_POLL_MS)).await;
        }
        Ok(calls)
    }
    async fn counters(&self) -> Result<Vec<(usize, Value)>> {
        let processes = self
            .processes
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        join_all(
            processes
                .into_iter()
                .map(|p| async move { Ok((p.slot, p.call(Command::Counters).await?)) }),
        )
        .await
        .into_iter()
        .collect()
    }
    async fn stop(&self) -> Result<StoppedWriters> {
        let processes = self
            .processes
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut exited = Vec::new();
        let mut errors = Vec::new();
        for process in processes {
            match process.stop().await {
                Ok(_) => exited.push(true),
                Err(error) => {
                    exited.push(false);
                    errors.push(error.to_string());
                }
            }
        }
        ensure!(
            errors.is_empty(),
            "failed to stop writers: {}",
            errors.join("; ")
        );
        StoppedWriters::confirm(&exited)
    }
}

pub(crate) async fn run(args: RunArgs) -> Result<i32> {
    if let Some(hours) = args.hours {
        ensure!(
            hours.is_finite() && hours > 0.0,
            "hours must be positive and finite"
        );
    }
    ensure!(args.rounds > 0, "rounds must be positive");
    validate_faults(&args.faults)?;
    let seed = args.seed.unwrap_or_else(rand::random);
    println!("seed={seed}");
    let name = format!("run-{}-{seed}", xmtp_common::time::now_ns());
    let root = args.directory.join(name);
    let mut ledger = RunLedger::new(&root, Limits::default())?;
    ledger.begin_round(0)?;
    let logs = Arc::new(Mutex::new(crate::ledger::private_file(
        &ledger.root().join("rounds/0/logs.jsonl"),
        true,
    )?));
    let network = NetworkController::create(PROXY_SLOTS).await?;
    let prepared = (|| -> Result<Vec<InstanceConfig>> {
        let endpoints = (0..PROXY_SLOTS)
            .map(|slot| network.endpoint(slot))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let configs = population::configs(ledger.root(), seed, &endpoints)?;
        ledger.write_json("proxies.json", &serde_json::to_value(network.records())?)?;
        ledger.write_status(&json!({"seed":seed,"phase":"setup","round":0}))?;
        Ok(configs)
    })();
    let configs = match prepared {
        Ok(configs) => configs,
        Err(error) => {
            network
                .shutdown()
                .await
                .with_context(|| format!("remove proxies after setup failed: {error:#}"))?;
            return Err(error);
        }
    };
    let supervisor = Arc::new(Supervisor {
        network,
        configs: Mutex::new(configs),
        processes: RwLock::new(BTreeMap::new()),
        ledger: Mutex::new(ledger),
        logs,
        registration: tokio::sync::Mutex::new(()),
    });
    let result = tokio::select! {result=run_rounds(&supervisor,&args,seed)=>result,signal=tokio::signal::ctrl_c()=>signal.map(|_|130).map_err(Into::into)};
    // Cleanup happens for success, interruption, partial setup, and harness errors.
    let cleared = supervisor.network.clear_all().await;
    let stopped = supervisor.stop().await;
    let removed = supervisor.network.shutdown().await;
    let had_writers = !supervisor.processes.read().await.is_empty();
    let cleanup = (|| -> Result<()> {
        cleared?;
        removed?;
        if had_writers {
            stopped?;
        }
        Ok(())
    })();
    let result = match (result, cleanup) {
        (Ok(code), Ok(())) => Ok(code),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => {
            Err(error.context(format!("cleanup also failed: {cleanup:#}")))
        }
    };
    {
        let ledger = supervisor.ledger.lock().expect("ledger mutex");
        let mut status: Value = serde_json::from_slice(&crate::ledger::read_bounded(
            &ledger.root().join("status.json"),
            4 * 1024 * 1024,
        )?)?;
        status["phase"] = json!("stopped");
        match &result {
            Err(error) => {
                status["verdict"] = json!("HARNESS");
                status["error"] = json!(format!("{error:#}"));
            }
            Ok(130) => status["verdict"] = json!("INTERRUPTED"),
            _ => {}
        }
        ledger.write_status(&status)?;
    }
    result
}

async fn run_rounds(s: &Arc<Supervisor>, args: &RunArgs, seed: u64) -> Result<i32> {
    for slot in 0..=SHARED_SLOT {
        s.start(slot).await?;
    }
    let processes = s.processes.read().await;
    let inboxes = (0..INBOXES)
        .map(|slot| processes[&slot].inbox_id.clone())
        .collect::<Vec<_>>();
    drop(processes);
    for (owner, members) in [(0, vec![1, 2]), (3, vec![4, 5]), (0, vec![1, 2, 3, 4, 5])] {
        s.operation(
            owner,
            Operation::Create {
                members: members.into_iter().map(|i| inboxes[i].clone()).collect(),
            },
        )
        .await?;
    }
    let mut snapshots = s.checkpoint().await?;
    let mut scheduler = Scheduler::new(seed);
    let mut counters = crate::counters::Counters::default();
    counters.observe(&s.counters().await?);
    let mut oracle = Oracle::new(args.strict, RetryBudgets::default());
    let run_start = Instant::now();
    let deadline = args
        .hours
        .map(|hours| Duration::from_secs_f64(hours * 3600.0));
    let mut round = 0;
    loop {
        if deadline.map_or(round >= args.rounds, |d| run_start.elapsed() >= d) {
            break;
        }
        round += 1;
        {
            let mut ledger = s.ledger.lock().expect("ledger mutex");
            ledger.enforce_healthy_bounds()?;
            ledger.begin_round(round)?;
            *s.logs.lock().expect("logs mutex") = crate::ledger::private_file(
                &ledger.root().join(format!("rounds/{round}/logs.jsonl")),
                true,
            )?;
            ledger.write_status(&json!({"seed":seed,"round":round,"phase":"chaos"}))?;
        }
        let population = {
            let configs = s.configs.lock().expect("configs mutex");
            snapshots
                .iter()
                .map(|i| InstanceView {
                    slot: i.instance,
                    inbox_index: configs[i.instance].inbox_index,
                    inbox_id: i.inbox_id.clone(),
                    groups: i
                        .groups
                        .iter()
                        .filter(|g| g.active)
                        .map(|g| g.group_id.clone())
                        .collect(),
                })
                .collect::<Vec<_>>()
        };
        let groups = snapshots
            .iter()
            .flat_map(|i| i.groups.iter())
            .filter(|g| g.active)
            .map(|g| {
                (
                    g.group_id.clone(),
                    GroupView {
                        id: g.group_id.clone(),
                        members: g
                            .members
                            .iter()
                            .map(|m| m.inbox_id.clone())
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>()
            .into_values()
            .collect::<Vec<_>>();
        let mut schedule = scheduler.next(round, &population, &groups);
        schedule
            .faults
            .retain(|f| fault_selected(&args.faults, &f.kind));
        s.record("schedule", serde_json::to_value(&schedule)?)?;
        s.ledger.lock().expect("ledger mutex").write_status(
            &json!({"seed":seed,"round":round,"phase":"chaos","schedule":schedule}),
        )?;
        let began = Instant::now();
        let operations = async {
            let mut counts = (0, 0);
            for (index, burst) in schedule.bursts.iter().enumerate() {
                let (ok, err) = s
                    .burst(
                        round,
                        index,
                        burst,
                        began + Duration::from_secs(CHAOS_SECONDS),
                    )
                    .await?;
                counts.0 += ok;
                counts.1 += err;
            }
            Ok::<_, anyhow::Error>(counts)
        };
        let faults = async {
            for result in join_all(schedule.faults.iter().map(|f| s.fault(round, f, began))).await {
                result?;
            }
            Ok::<_, anyhow::Error>(())
        };
        let ((ok, errors), ()) = tokio::try_join!(operations, faults)?;
        s.clear().await?;
        for process in s.processes.read().await.values() {
            process.call(Command::Drain).await?;
        }
        s.handover().await?;
        snapshots = s.checkpoint().await?;
        let rollcall = s.rollcall(round, seed, &snapshots).await?;
        snapshots = s.checkpoint().await?;
        let result = oracle.evaluate(
            round,
            run_start.elapsed().as_millis() as u64,
            &snapshots,
            &rollcall,
        );
        let observations = s.counters().await?;
        let streams = observations
            .iter()
            .map(|(instance, value)| json!({"instance":instance,"diagnostics":value["stream"]}))
            .collect::<Vec<_>>();
        let round_counters = counters.observe(&observations);
        let conflicts = round_counters.own_commit_epoch_conflicts;
        let welcome_retries = round_counters.welcome_retries;
        let mut summary = json!({"seed":seed,"round":round,"phase":"checkpoint","verdict":result.verdict,"check":result,"installations":snapshots,"rollcall":rollcall,"schedule":schedule,"proxies":s.network.records(),"counters":round_counters,"ops":ok+errors,"ok":ok,"errors":errors,"conflicts":conflicts,"welcome_retries":welcome_retries});
        crate::report::bound_history(&mut summary);
        summary["streams"] = json!(streams);
        let warnings = result
            .findings
            .iter()
            .filter(|f| f.verdict == crate::check::Verdict::Warn)
            .count();
        {
            let ledger = s.ledger.lock().expect("ledger mutex");
            ledger.write_status(&summary)?;
            ledger.enforce_bounds()?;
        }
        println!(
            "round {round} ops={} ok={ok} err={errors} faults={} kinds={} bursts={} conflicts={conflicts} wretry={welcome_retries} warn={warnings} check={} {}s",
            ok + errors,
            schedule.faults.len(),
            schedule
                .faults
                .iter()
                .map(|fault| fault.kind.as_str())
                .collect::<Vec<_>>()
                .join(","),
            schedule.bursts.len(),
            result.verdict,
            began.elapsed().as_secs()
        );
        if result.verdict == crate::check::Verdict::Harness {
            bail!("oracle could not produce a verdict: {:?}", result.findings);
        }
        if result.violation {
            let stopped = s.stop().await?;
            let active = s
                .processes
                .read()
                .await
                .keys()
                .copied()
                .collect::<BTreeSet<_>>();
            let configs = s.configs.lock().expect("configs mutex");
            let databases = configs
                .iter()
                .filter(|c| active.contains(&c.slot) && c.slot != SHARED_SLOT)
                .map(|c| DatabaseCopy {
                    source: c.database.clone(),
                    name: format!("instance-{}", c.slot),
                    key: json!(c.database_key),
                })
                .collect::<Vec<_>>();
            let path = crate::evidence::write_bundle(
                &s.ledger.lock().expect("ledger mutex"),
                &stopped,
                &summary,
                &databases,
            )?;
            println!("bundle={}", path.display());
            return Ok(2);
        }
    }
    Ok(0)
}

fn fault_selected(selection: &str, kind: &str) -> bool {
    selection.split(',').any(|part| match part {
        "all" => true,
        "none" => false,
        "network" => serde_json::from_value::<NetworkFault>(json!(kind)).is_ok(),
        "disk" => kind.starts_with("disk_") || kind == "connection_loss",
        "crash" => kind == "process_kill",
        _ => part == kind,
    })
}
fn validate_faults(selection: &str) -> Result<()> {
    for part in selection.split(',') {
        ensure!(
            matches!(
                part,
                "all"
                    | "none"
                    | "network"
                    | "disk"
                    | "crash"
                    | "disk_locked"
                    | "disk_io"
                    | "disk_full"
                    | "disk_after_call"
                    | "connection_loss"
                    | "process_kill"
            ) || serde_json::from_value::<NetworkFault>(json!(part)).is_ok(),
            "unknown fault set {part}"
        );
    }
    Ok(())
}

pub(crate) fn newest(path: &Path) -> PathBuf {
    if path.join("status.json").is_file() {
        return path.to_owned();
    }
    std::fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.file_type().is_ok_and(|t| t.is_dir())
                && entry.file_name().to_string_lossy().starts_with("run-")
        })
        .max_by_key(|entry| entry.file_name())
        .map_or_else(|| path.to_owned(), |entry| entry.path())
}
