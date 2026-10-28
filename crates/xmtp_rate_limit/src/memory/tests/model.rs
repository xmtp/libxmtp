use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::MAX_LIMITS;

const CALLERS: [&[u8]; 6] = [b"", b"a", b"b", b"api_key:bridge-bot", &[0; 9], &[0xff; 33]];
const CATEGORIES: [u8; 3] = [0, 1, 255];

/// One TAT per limit for every (caller, category) ever charged, with no
/// capacity bound, applying the admission arithmetic directly.
#[derive(Default)]
struct Model {
    tats: BTreeMap<(usize, usize), [u64; MAX_LIMITS]>,
}

impl Model {
    fn spend(&mut self, at: (usize, usize), limits: &[Limit], cost: u32, now: u64) -> Decision {
        let limits = &limits[..limits.len().min(MAX_LIMITS)];
        if cost == 0 || limits.is_empty() {
            return Decision::Allowed;
        }
        let tats = self.tats.get(&at).copied().unwrap_or_default();
        let (cost, now) = (u128::from(cost), u128::from(now));
        let mut deny = 0u128;
        let mut next = [0u64; MAX_LIMITS];
        for (i, limit) in limits.iter().enumerate() {
            let tat = u128::from(tats[i]);
            let excess = tat.saturating_sub(now);
            let (count, window) = (u128::from(limit.count), limit.window.as_nanos());
            let add = cost * window / count;
            if cost > count {
                if excess > 0 {
                    deny = deny.max(excess);
                }
            } else if excess + add > window {
                deny = deny.max(excess + add - window);
            }
            next[i] = u64::try_from(now + excess + add).unwrap_or(u64::MAX);
        }
        if deny > 0 {
            let millis = deny.div_ceil(u128::from(MILLI));
            return Decision::Denied {
                retry_after: Duration::from_millis(u64::try_from(millis).unwrap_or(u64::MAX)),
            };
        }
        self.tats.insert(at, next);
        Decision::Allowed
    }
}

#[derive(Debug, Clone)]
struct Op {
    advance: u64,
    caller: usize,
    category: usize,
    cost: u32,
}

fn op_strategy() -> impl Strategy<Value = Op> {
    let cost = prop_oneof![8 => 0u32..=4, 4 => 0u32..=100, 1 => Just(u32::MAX)];
    (
        advance_strategy(),
        0..CALLERS.len(),
        0..CATEGORIES.len(),
        cost,
    )
        .prop_map(|(advance, caller, category, cost)| Op {
            advance,
            caller,
            category,
            cost,
        })
}

/// A limit list per (caller, category), as a configuration resolves one, with
/// an occasional extreme count or window.
fn limit_table_strategy() -> impl Strategy<Value = Vec<Vec<Limit>>> {
    let extreme = proptest::collection::vec(
        (
            prop_oneof![Just(1u32), Just(u32::MAX)],
            prop_oneof![Just(1u64), Just(30 * 86_400 * SECOND), Just(u64::MAX)],
        ),
        1..=MAX_LIMITS,
    )
    .prop_map(|limits| {
        limits
            .into_iter()
            .map(|(count, window)| Limit {
                count,
                window: Duration::from_nanos(window),
            })
            .collect()
    });
    let limits = prop_oneof![8 => limits_strategy(0..=MAX_LIMITS), 1 => extreme];
    proptest::collection::vec(limits, CALLERS.len() * CATEGORIES.len())
}

/// Runs the operations on a limiter of `budget` bytes and on the model, and
/// requires the same decision, retry delay included, until the limiter first
/// forgets a caller that was not at full allowance. Counts the denials it
/// compared in `denials`.
fn check(
    budget: usize,
    limit_table: &[Vec<Limit>],
    ops: &[Op],
    denials: &AtomicUsize,
) -> Result<(), TestCaseError> {
    let clock = ManualClock::default();
    let limiter = MemoryLimiter::with_clock(budget, clock.clone());
    let mut model = Model::default();
    for op in ops {
        clock.advance(op.advance);
        let limits = &limit_table[op.caller * CATEGORIES.len() + op.category];
        let expected = model.spend((op.caller, op.category), limits, op.cost, clock.now());
        let decision = spend_as(
            &limiter,
            CALLERS[op.caller],
            CATEGORIES[op.category],
            limits,
            op.cost,
        );
        if limiter.evictions() > 0 {
            break;
        }
        prop_assert_eq!(decision, expected, "{:?} at {}", op, clock.now());
        if decision != Decision::Allowed {
            denials.fetch_add(1, Ordering::Relaxed);
        }
    }
    Ok(())
}

#[xmtp_common::test]
fn decisions_match_an_unbounded_model() {
    let denials = AtomicUsize::new(0);
    let ops = proptest::collection::vec(op_strategy(), 1..300);
    runner()
        .run(&(limit_table_strategy(), ops), |(limit_table, ops)| {
            check(BUDGET, &limit_table, &ops, &denials)
        })
        .unwrap();
    assert!(denials.into_inner() > 0, "no denial was compared");
}

/// A table of one set: callers reuse slots at full allowance in place, which
/// changes no decision.
#[xmtp_common::test]
fn decisions_match_an_unbounded_model_in_one_set() {
    type ParkingTable = crate::table::Table<parking_lot::Mutex<Box<[crate::table::Set]>>>;
    let one_set = size_of::<crate::table::Set>() + ParkingTable::SHARD_BYTES;
    let denials = AtomicUsize::new(0);
    let ops = proptest::collection::vec(op_strategy(), 1..300);
    runner()
        .run(&(limit_table_strategy(), ops), |(limit_table, ops)| {
            check(one_set, &limit_table, &ops, &denials)
        })
        .unwrap();
    assert!(denials.into_inner() > 0, "no denial was compared");
}
