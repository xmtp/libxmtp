//! Seeded operation schedules and independent fault windows.
use crate::{protocol::Operation, recipes};
use rand::{RngExt, SeedableRng, seq::SliceRandom};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

pub(crate) const OPERATIONS_PER_ROUND: usize = 24;
pub(crate) const MAX_GROUPS: usize = 8;
pub(crate) const MAX_INSTALLATIONS: usize = 18;
const MAX_INSTALLATIONS_PER_INBOX: usize = 3;
const SAME_EPOCH_BURSTS: usize = 3;
const MAX_FAULTS: usize = 3;
const MIN_WINDOW_MS: u64 = 500;
const MAX_WINDOW_MS: u64 = 8_000;
const FAULT_SEED_DOMAIN: u64 = 0xb89b_430f_1fe7_3ad6;
const OPERATION_KINDS: u64 = 12;
const FAULT_KINDS: &[&str] = &[
    "disconnect",
    "latency",
    "bandwidth",
    "timeout",
    "limit_data",
    "reset_peer",
    "slicer",
    "backend_pause",
    "disk_locked",
    "disk_io",
    "disk_full",
    "disk_after_call",
    "connection_loss",
    "process_kill",
];

#[derive(Clone, Debug)]
pub(crate) struct InstanceView {
    pub slot: usize,
    pub inbox_index: usize,
    pub inbox_id: String,
    /// Groups present in this installation's local state.
    pub groups: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupView {
    pub id: String,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ScheduledOperation {
    pub instance: usize,
    pub operation: Operation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Burst {
    pub recipe: String,
    /// Operations for the same instance execute in this order.
    pub operations: Vec<ScheduledOperation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FaultWindow {
    pub instance: usize,
    pub kind: String,
    pub start_ms: u64,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RoundSchedule {
    pub bursts: Vec<Burst>,
    pub faults: Vec<FaultWindow>,
}

pub(crate) struct Scheduler {
    rng: ChaCha8Rng,
    fault_rng: ChaCha8Rng,
}

impl Scheduler {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
            fault_rng: ChaCha8Rng::seed_from_u64(seed ^ FAULT_SEED_DOMAIN),
        }
    }

    pub fn next(
        &mut self,
        round: u64,
        population: &[InstanceView],
        groups: &[GroupView],
    ) -> RoundSchedule {
        if population.is_empty() {
            return RoundSchedule {
                bursts: Vec::new(),
                faults: Vec::new(),
            };
        }
        let mut hot: Vec<_> = groups
            .iter()
            .filter(|group| members(population, group).len() >= 2)
            .collect();
        hot.shuffle(&mut self.rng);
        let hot_count = self.rng.random_range(1..=2);
        hot.truncate(hot_count);
        let mut bursts = Vec::new();
        for index in 0..SAME_EPOCH_BURSTS {
            if let Some(group) = hot.get(index % hot.len().max(1)) {
                let mut actors = members(population, group);
                actors.shuffle(&mut self.rng);
                bursts.push(recipes::same_epoch(round, index, group, &actors));
            }
        }
        if let Some(group) = hot.first() {
            let mut actors = members(population, group);
            actors.shuffle(&mut self.rng);
            bursts.extend(recipes::recipe(
                round,
                group,
                &actors,
                population,
                &mut self.rng,
            ));
        }
        let mut count: usize = bursts.iter().map(|burst| burst.operations.len()).sum();
        let mut scheduled_creates = 0;
        let mut scheduled_installations: Vec<_> = bursts
            .iter()
            .flat_map(|burst| &burst.operations)
            .filter_map(|scheduled| match scheduled.operation {
                Operation::NewInstallation { inbox_index } => Some(inbox_index),
                _ => None,
            })
            .collect();
        let mut tail = Vec::new();
        while count < OPERATIONS_PER_ROUND {
            let kind = if tail.is_empty() {
                round % OPERATION_KINDS
            } else {
                self.rng.random_range(0..OPERATION_KINDS)
            };
            let target = hot.get(self.rng.random_range(0..hot.len().max(1))).copied();
            let operation = self.operation(
                (round, count),
                kind,
                population,
                target,
                groups.len() + scheduled_creates,
                &scheduled_installations,
            );
            if matches!(operation.operation, Operation::Create { .. }) {
                scheduled_creates += 1;
            }
            if let Operation::NewInstallation { inbox_index } = operation.operation {
                scheduled_installations.push(inbox_index);
            }
            tail.push(operation);
            count += 1;
        }
        if !tail.is_empty() {
            bursts.push(Burst {
                recipe: "mixed_operations".into(),
                operations: tail,
            });
        }
        RoundSchedule {
            bursts,
            faults: self.faults(population),
        }
    }

    fn operation(
        &mut self,
        position: (u64, usize),
        kind: u64,
        population: &[InstanceView],
        group: Option<&GroupView>,
        group_count: usize,
        planned_installations: &[usize],
    ) -> ScheduledOperation {
        let (round, index) = position;
        let any = &population[self.rng.random_range(0..population.len())];
        if kind == 0 && group_count < MAX_GROUPS {
            let mut inboxes: Vec<_> = population
                .iter()
                .map(|instance| instance.inbox_id.clone())
                .collect();
            inboxes.sort();
            inboxes.dedup();
            inboxes.retain(|inbox| inbox != &any.inbox_id);
            inboxes.shuffle(&mut self.rng);
            // Keep a nonmember available for later join races.
            inboxes.truncate(2);
            return scheduled(any, Operation::Create { members: inboxes });
        }
        if kind == 8 && population.len() + planned_installations.len() < MAX_INSTALLATIONS {
            let mut candidates: Vec<_> = population
                .iter()
                .filter(|candidate| {
                    population
                        .iter()
                        .filter(|instance| instance.inbox_index == candidate.inbox_index)
                        .count()
                        + planned_installations
                            .iter()
                            .filter(|index| **index == candidate.inbox_index)
                            .count()
                        < MAX_INSTALLATIONS_PER_INBOX
                })
                .collect();
            candidates.shuffle(&mut self.rng);
            if let Some(actor) = candidates.first() {
                return scheduled(
                    actor,
                    Operation::NewInstallation {
                        inbox_index: actor.inbox_index,
                    },
                );
            }
        }
        if kind == 7 {
            return scheduled(any, Operation::SyncAll);
        }
        if kind == 9 {
            return scheduled(any, Operation::RestartStream);
        }
        let Some(group) = group else {
            return scheduled(any, Operation::SyncAll);
        };
        let actors = members(population, group);
        let actor = actors[self.rng.random_range(0..actors.len())];
        let other = actors
            .iter()
            .find(|candidate| candidate.inbox_id != actor.inbox_id);
        let operation = match kind {
            1 => population
                .iter()
                .find(|candidate| !group.members.contains(&candidate.inbox_id))
                .map(|candidate| Operation::Add {
                    group: group.id.clone(),
                    inbox: candidate.inbox_id.clone(),
                }),
            2 if actors.len() > 2 => other.map(|candidate| Operation::Remove {
                group: group.id.clone(),
                inbox: candidate.inbox_id.clone(),
            }),
            3 => other.map(|candidate| Operation::Readd {
                group: group.id.clone(),
                inbox: candidate.inbox_id.clone(),
            }),
            4 => Some(Operation::Metadata {
                group: group.id.clone(),
                value: format!("round-{round}-op-{index}"),
            }),
            5 => Some(Operation::Send {
                group: group.id.clone(),
                token: format!("chaos-{round}-{index}-{}", actor.slot),
            }),
            6 => Some(Operation::Sync {
                group: group.id.clone(),
            }),
            10 => Some(Operation::Consent {
                group: group.id.clone(),
                state: self.rng.random_range(0..=2),
            }),
            11 => Some(Operation::UpdateInstallations {
                group: group.id.clone(),
            }),
            _ => None,
        }
        .unwrap_or_else(|| Operation::Metadata {
            group: group.id.clone(),
            value: format!("round-{round}-op-{index}"),
        });
        scheduled(actor, operation)
    }

    fn faults(&mut self, population: &[InstanceView]) -> Vec<FaultWindow> {
        let count = self.fault_rng.random_range(0..=MAX_FAULTS);
        let mut slots: Vec<_> = population.iter().map(|instance| instance.slot).collect();
        slots.shuffle(&mut self.fault_rng);
        // Distinct slots avoid one window clearing another window's fault.
        let mut available_kinds = FAULT_KINDS.to_vec();
        slots
            .into_iter()
            .take(count)
            .map(|instance| {
                let selected = self.fault_rng.random_range(0..available_kinds.len());
                let kind = available_kinds[selected];
                if kind == "backend_pause" {
                    // A global pause must have one owner until it is cleared.
                    available_kinds.remove(selected);
                }
                FaultWindow {
                    instance,
                    kind: kind.into(),
                    start_ms: self.fault_rng.random_range(MIN_WINDOW_MS..=MAX_WINDOW_MS),
                    duration_ms: self.fault_rng.random_range(MIN_WINDOW_MS..=MAX_WINDOW_MS),
                }
            })
            .collect()
    }
}

/// One local installation for each member inbox participates in a commit burst.
pub(crate) fn members<'a>(
    population: &'a [InstanceView],
    group: &GroupView,
) -> Vec<&'a InstanceView> {
    let mut actors = Vec::new();
    for instance in population {
        if instance.groups.contains(&group.id)
            && group.members.contains(&instance.inbox_id)
            && !actors
                .iter()
                .any(|actor: &&InstanceView| actor.inbox_id == instance.inbox_id)
        {
            actors.push(instance);
        }
    }
    actors
}

pub(crate) fn scheduled(instance: &InstanceView, operation: Operation) -> ScheduledOperation {
    ScheduledOperation {
        instance: instance.slot,
        operation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn population() -> (Vec<InstanceView>, Vec<GroupView>) {
        let groups = vec![
            GroupView {
                id: "first".into(),
                members: vec!["inbox-0".into(), "inbox-1".into(), "inbox-2".into()],
            },
            GroupView {
                id: "second".into(),
                members: vec!["inbox-3".into(), "inbox-4".into(), "inbox-5".into()],
            },
        ];
        let population = (0..6)
            .map(|slot| InstanceView {
                slot,
                inbox_index: slot,
                inbox_id: format!("inbox-{slot}"),
                groups: vec![if slot < 3 { "first" } else { "second" }.into()],
            })
            .collect();
        (population, groups)
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn schedules_repeat_and_cover_recipes_and_operations() {
        let (population, groups) = population();
        let mut first = Scheduler::new(0x5eed);
        let mut second = Scheduler::new(0x5eed);
        let mut recipes = BTreeSet::new();
        let mut operations = BTreeSet::new();
        for round in 0..50 {
            let schedule = first.next(round, &population, &groups);
            assert_eq!(schedule, second.next(round, &population, &groups));
            assert_eq!(
                schedule
                    .bursts
                    .iter()
                    .map(|burst| burst.operations.len())
                    .sum::<usize>(),
                OPERATIONS_PER_ROUND
            );
            assert!(
                schedule
                    .bursts
                    .iter()
                    .filter(|burst| burst.recipe == "same_epoch_contention")
                    .count()
                    >= SAME_EPOCH_BURSTS
            );
            for burst in schedule.bursts {
                recipes.insert(burst.recipe);
                for scheduled in burst.operations {
                    operations.insert(
                        serde_json::to_value(scheduled.operation)?["kind"]
                            .as_str()?
                            .to_owned(),
                    );
                }
            }
        }
        for recipe in [
            "join_race",
            "same_epoch_contention",
            "remove_and_readd",
            "remove_with_pending_intent",
            "installation_during_commits",
            "sync_racing_stream",
        ] {
            assert!(recipes.contains(recipe), "missing recipe {recipe}");
        }
        for operation in [
            "create",
            "add",
            "remove",
            "readd",
            "metadata",
            "send",
            "pending_send",
            "sync",
            "sync_all",
            "new_installation",
            "restart_stream",
            "consent",
            "update_installations",
        ] {
            assert!(
                operations.contains(operation),
                "missing operation {operation}"
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn targets_are_local_and_creation_stays_bounded() {
        let (population, groups) = population();
        let mut scheduler = Scheduler::new(42);
        for round in 0..50 {
            let schedule = scheduler.next(round, &population, &groups);
            let mut creates = groups.len();
            let mut installs = population.len();
            for scheduled in schedule
                .bursts
                .into_iter()
                .flat_map(|burst| burst.operations)
            {
                let local = &population[scheduled.instance];
                match scheduled.operation {
                    Operation::Create { .. } => creates += 1,
                    Operation::NewInstallation { .. } => installs += 1,
                    Operation::Add { group, .. }
                    | Operation::Remove { group, .. }
                    | Operation::Readd { group, .. }
                    | Operation::Metadata { group, .. }
                    | Operation::Send { group, .. }
                    | Operation::PendingSend { group, .. }
                    | Operation::Sync { group }
                    | Operation::Consent { group, .. }
                    | Operation::UpdateInstallations { group } => {
                        assert!(local.groups.contains(&group))
                    }
                    Operation::SyncAll | Operation::RestartStream => {}
                }
            }
            assert!(creates <= MAX_GROUPS);
            assert!(installs <= MAX_INSTALLATIONS);
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn faults_are_independent_bounded_windows() {
        let (population, groups) = population();
        let mut with_groups = Scheduler::new(9);
        let mut without_groups = Scheduler::new(9);
        let mut kinds = BTreeSet::new();
        for round in 0..200 {
            let schedule = with_groups.next(round, &population, &groups);
            let other = without_groups.next(round, &population, &[]);
            assert_eq!(schedule.faults, other.faults);
            assert!(
                schedule
                    .faults
                    .iter()
                    .filter(|fault| fault.kind == "backend_pause")
                    .count()
                    <= 1
            );
            let mut events = Vec::new();
            let mut slots = BTreeSet::new();
            for fault in schedule.faults {
                assert!(slots.insert(fault.instance));
                assert!((MIN_WINDOW_MS..=MAX_WINDOW_MS).contains(&fault.start_ms));
                assert!((MIN_WINDOW_MS..=MAX_WINDOW_MS).contains(&fault.duration_ms));
                kinds.insert(fault.kind);
                events.push((fault.start_ms, 1_i32));
                events.push((fault.start_ms + fault.duration_ms, -1_i32));
            }
            events.sort();
            let mut active = 0;
            for (_, change) in events {
                active += change;
                assert!((0..=MAX_FAULTS as i32).contains(&active));
            }
            assert_eq!(active, 0);
        }
        assert_eq!(kinds.len(), FAULT_KINDS.len());
    }
}
