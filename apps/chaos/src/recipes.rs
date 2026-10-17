//! Short scripts that concentrate commits in one group.
use crate::{
    protocol::Operation,
    schedule::{Burst, GroupView, InstanceView, MAX_INSTALLATIONS, scheduled},
};
use rand::{RngExt, seq::SliceRandom};
use rand_chacha::ChaCha8Rng;

const RECIPE_COUNT: u64 = 8;
const INSTALLATIONS_PER_INBOX: usize = 3;
const MAX_RECIPE_COMMITTERS: usize = 4;

pub(crate) fn same_epoch(
    round: u64,
    burst: usize,
    group: &GroupView,
    actors: &[&InstanceView],
) -> Burst {
    Burst {
        recipe: "same_epoch_contention".into(),
        operations: actors
            .iter()
            .map(|actor| {
                scheduled(
                    actor,
                    Operation::Metadata {
                        group: group.id.clone(),
                        value: format!("contention-{round}-{burst}-{}", actor.slot),
                    },
                )
            })
            .collect(),
    }
}

pub(crate) fn recipe(
    round: u64,
    group: &GroupView,
    actors: &[&InstanceView],
    population: &[InstanceView],
    rng: &mut ChaCha8Rng,
) -> Vec<Burst> {
    let a = actors[0];
    let b = actors[1];
    let mut operations = Vec::new();
    let mut preparation = Vec::new();
    let name = match round % RECIPE_COUNT {
        0 => {
            let mut candidates: Vec<_> = population
                .iter()
                .filter(|instance| !group.members.contains(&instance.inbox_id))
                .collect();
            candidates.shuffle(rng);
            let joiner = candidates.first().copied().unwrap_or(b);
            if candidates.is_empty() {
                preparation.push(Burst {
                    recipe: "join_race_prepare".into(),
                    operations: vec![scheduled(
                        a,
                        Operation::Remove {
                            group: group.id.clone(),
                            inbox: joiner.inbox_id.clone(),
                        },
                    )],
                });
            }
            operations.push(scheduled(
                a,
                Operation::Add {
                    group: group.id.clone(),
                    inbox: joiner.inbox_id.clone(),
                },
            ));
            operations.push(scheduled(
                a,
                Operation::Metadata {
                    group: group.id.clone(),
                    value: format!("join-{round}-a"),
                },
            ));
            if let Some(c) = actors
                .iter()
                .find(|actor| actor.inbox_id != a.inbox_id && actor.inbox_id != joiner.inbox_id)
            {
                operations.push(scheduled(
                    c,
                    Operation::Metadata {
                        group: group.id.clone(),
                        value: format!("join-{round}-c"),
                    },
                ));
            }
            operations.push(scheduled(joiner, Operation::SyncAll));
            "join_race"
        }
        1 => return vec![same_epoch(round, 3, group, actors)],
        2 => {
            operations.push(scheduled(
                a,
                Operation::Readd {
                    group: group.id.clone(),
                    inbox: b.inbox_id.clone(),
                },
            ));
            operations.push(scheduled(
                b,
                Operation::Sync {
                    group: group.id.clone(),
                },
            ));
            "remove_and_readd"
        }
        3 => {
            operations.push(scheduled(
                b,
                Operation::PendingSend {
                    group: group.id.clone(),
                    token: format!("pending-{round}-{}", b.slot),
                },
            ));
            operations.push(scheduled(
                a,
                Operation::Remove {
                    group: group.id.clone(),
                    inbox: b.inbox_id.clone(),
                },
            ));
            "remove_with_pending_intent"
        }
        4 => {
            let mut candidates: Vec<_> = actors
                .iter()
                .filter(|actor| {
                    population
                        .iter()
                        .filter(|instance| instance.inbox_index == actor.inbox_index)
                        .count()
                        < INSTALLATIONS_PER_INBOX
                })
                .collect();
            candidates.shuffle(rng);
            if population.len() < MAX_INSTALLATIONS
                && let Some(actor) = candidates.first()
            {
                operations.push(scheduled(
                    actor,
                    Operation::NewInstallation {
                        inbox_index: actor.inbox_index,
                    },
                ));
            } else {
                operations.push(scheduled(
                    a,
                    Operation::UpdateInstallations {
                        group: group.id.clone(),
                    },
                ));
            }
            for actor in actors.iter().take(MAX_RECIPE_COMMITTERS) {
                operations.push(scheduled(
                    actor,
                    Operation::Metadata {
                        group: group.id.clone(),
                        value: format!("installation-{round}-{}", actor.slot),
                    },
                ));
            }
            "installation_during_commits"
        }
        6 => {
            // A removal sweeps the removed inbox's profile entry while that
            // inbox writes it and a third member's key update races both.
            let remover = actors
                .iter()
                .find(|actor| group.is_admin(&actor.inbox_id))
                .unwrap_or(&a);
            let Some(target) = actors.iter().find(|actor| {
                actor.inbox_id != remover.inbox_id && !group.super_admins.contains(&actor.inbox_id)
            }) else {
                return vec![same_epoch(round, 3, group, actors)];
            };
            operations.push(scheduled(
                target,
                Operation::DisplayName {
                    group: group.id.clone(),
                    value: Some(format!("profile-{round}-{}", target.slot)),
                },
            ));
            operations.push(scheduled(
                remover,
                Operation::Remove {
                    group: group.id.clone(),
                    inbox: target.inbox_id.clone(),
                },
            ));
            if let Some(c) = actors.iter().find(|actor| {
                actor.inbox_id != remover.inbox_id && actor.inbox_id != target.inbox_id
            }) {
                operations.push(scheduled(
                    c,
                    Operation::KeyUpdate {
                        group: group.id.clone(),
                    },
                ));
            }
            "profile_during_removal"
        }
        7 => {
            // A leave request races profile writes until an admin commits the removal.
            let Some(leaver) = actors
                .iter()
                .find(|actor| !group.super_admins.contains(&actor.inbox_id))
            else {
                return vec![same_epoch(round, 3, group, actors)];
            };
            operations.push(scheduled(
                leaver,
                Operation::Leave {
                    group: group.id.clone(),
                },
            ));
            for actor in actors
                .iter()
                .filter(|actor| actor.inbox_id != leaver.inbox_id)
                .take(MAX_RECIPE_COMMITTERS)
            {
                operations.push(scheduled(
                    actor,
                    Operation::DisplayName {
                        group: group.id.clone(),
                        value: Some(format!("leave-{round}-{}", actor.slot)),
                    },
                ));
            }
            "leave_during_commits"
        }
        _ => {
            let actor = actors[rng.random_range(0..actors.len())];
            operations.push(scheduled(actor, Operation::RestartStream));
            operations.push(scheduled(
                actor,
                Operation::Sync {
                    group: group.id.clone(),
                },
            ));
            operations.push(scheduled(
                a,
                Operation::Send {
                    group: group.id.clone(),
                    token: format!("stream-race-{round}-{}", a.slot),
                },
            ));
            "sync_racing_stream"
        }
    };
    preparation.push(Burst {
        recipe: name.into(),
        operations,
    });
    preparation
}
