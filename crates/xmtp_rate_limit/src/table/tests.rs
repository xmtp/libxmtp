use std::time::Duration;

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

use super::*;

mod interleavings;

type ParkingTable = Table<parking_lot::Mutex<Box<[Set]>>>;

const SECOND: u64 = 1_000_000_000;
const MIB: usize = 1 << 20;

fn one_set() -> ParkingTable {
    Table::with_layout(1, 1)
}

fn key(i: usize) -> [u8; 8] {
    (i as u64).to_le_bytes()
}

fn spend(
    table: &ParkingTable,
    now: u64,
    key: &[u8],
    category: u8,
    limits: &[Limit],
    cost: u32,
) -> Decision {
    table.spend(
        now,
        &Spend {
            key,
            category,
            limits,
            cost,
        },
    )
}

fn latest(tats: Tats) -> u64 {
    tats.into_iter().max().unwrap_or(0)
}

#[xmtp_common::test]
fn the_layout_fills_the_budget() {
    let set_bytes = size_of::<Set>();
    for budget in [MIB, 3 * MIB + 17, 100 * MIB, 1024 * MIB, 65536 * MIB] {
        let (shards, sets) = layout(budget, ParkingTable::SHARD_BYTES);
        assert!(
            shards * ParkingTable::SHARD_BYTES + sets * set_bytes <= budget,
            "{budget}"
        );
        assert!(
            sets * WAYS * 100 >= budget / size_of::<Slot>() * 95,
            "{budget}"
        );
        assert_eq!(shards, MAX_SHARDS, "{budget}");
    }
    // Small budgets spread what they buy one set per shard.
    assert_eq!(layout(10 * (set_bytes + 24), 24), (10, 10));
    assert_eq!(layout(0, 24), (1, 1));
}

#[xmtp_common::test]
fn sets_spread_over_contiguous_shards() {
    let table = ParkingTable::with_layout(3, 10);
    let lens: Vec<usize> = table
        .shards
        .iter()
        .map(|shard| shard.with(|sets| sets.len()))
        .collect();
    assert_eq!(lens, [4, 3, 3]);
    assert_eq!(table.slots(), 10 * WAYS);
}

/// A set full of callers forgets the one whose largest TAT is smallest, so
/// every caller at full allowance goes before any caller with spend left.
#[xmtp_common::test]
fn a_full_set_forgets_the_caller_that_spent_least() {
    let limits = [
        Limit {
            count: 5,
            window: Duration::from_secs(10),
        },
        Limit {
            count: 50,
            window: Duration::from_secs(3600),
        },
    ];
    let spends = proptest::collection::vec((0..2 * SECOND, 1u32..=5), 1..4);
    let callers = proptest::collection::vec(spends, WAYS);
    let mut runner = TestRunner::new(Config::default());
    runner
        .run(&(callers, 0..120 * SECOND), |(callers, last_advance)| {
            let table = one_set();
            let mut now = 0;
            for (caller, spends) in callers.iter().enumerate() {
                for &(advance, cost) in spends {
                    now += advance;
                    spend(&table, now, &key(caller), 0, &limits, cost);
                }
            }
            prop_assert_eq!(table.evictions(), 0);
            now += last_advance;
            let before: Vec<Tats> = (0..WAYS)
                .map(|caller| {
                    table
                        .tats(0, &key(caller))
                        .expect("every caller has a slot")
                })
                .collect();
            let least = before.iter().copied().map(latest).min().unwrap_or(0);

            prop_assert_eq!(
                spend(&table, now, b"fresh", 0, &limits, 1),
                Decision::Allowed
            );

            let after: Vec<Option<Tats>> = (0..WAYS)
                .map(|caller| table.tats(0, &key(caller)))
                .collect();
            let forgotten: Vec<usize> = (0..WAYS)
                .filter(|&caller| after[caller].is_none())
                .collect();
            prop_assert_eq!(forgotten.len(), 1);
            prop_assert_eq!(latest(before[forgotten[0]]), least);
            for caller in (0..WAYS).filter(|&caller| caller != forgotten[0]) {
                prop_assert_eq!(after[caller], Some(before[caller]));
            }
            prop_assert!(table.tats(0, b"fresh").is_some());
            prop_assert_eq!(table.evictions(), u64::from(least > now));
            Ok(())
        })
        .unwrap();
}

/// The caller at full allowance is forgotten even when it was recorded last
/// and spent the most units.
#[xmtp_common::test]
fn a_full_set_reuses_a_slot_at_full_allowance_first() {
    let slow = [Limit {
        count: 1000,
        window: Duration::from_secs(3600),
    }];
    let fast = [Limit {
        count: 100,
        window: Duration::from_secs(1),
    }];
    let table = one_set();
    for caller in 0..WAYS - 1 {
        assert_eq!(
            spend(&table, 0, &key(caller), 1, &slow, 1),
            Decision::Allowed
        );
    }
    assert_eq!(spend(&table, 0, b"heavy", 2, &fast, 100), Decision::Allowed);
    let now = 2 * SECOND;

    assert_eq!(spend(&table, now, b"fresh", 1, &slow, 1), Decision::Allowed);

    assert_eq!(table.tats(2, b"heavy"), None);
    for caller in 0..WAYS - 1 {
        assert!(table.tats(1, &key(caller)).is_some());
    }
    assert_eq!(table.evictions(), 0);
}

// verifies: RATE-051
#[xmtp_common::test]
fn a_full_table_admits_fresh_callers_and_forgets_others() {
    const FLOOD: usize = 100;
    let limits = [Limit {
        count: 1,
        window: Duration::from_secs(3600),
    }];
    let table = one_set();
    for caller in 0..WAYS {
        assert_eq!(
            spend(&table, 0, &key(caller), 0, &limits, 1),
            Decision::Allowed
        );
        assert!(matches!(
            spend(&table, 0, &key(caller), 0, &limits, 1),
            Decision::Denied { .. }
        ));
    }

    for caller in WAYS..WAYS + FLOOD {
        // An oversized cost too: a caller with no state is at full allowance.
        let cost = if caller % 2 == 0 { 1 } else { 5 };
        assert_eq!(
            spend(&table, 0, &key(caller), 0, &limits, cost),
            Decision::Allowed
        );
    }

    assert_eq!(table.evictions(), FLOOD as u64);
    let newest = key(WAYS + FLOOD - 1);
    assert!(table.tats(0, &newest).is_some());
    assert!(matches!(
        spend(&table, 0, &newest, 0, &limits, 1),
        Decision::Denied { .. }
    ));
}

/// A spend that leaves the caller at full allowance records nothing, so it
/// forgets no one.
#[xmtp_common::test]
fn a_spend_that_stays_at_full_allowance_takes_no_slot() {
    let limits = [Limit {
        count: 1,
        window: Duration::from_secs(3600),
    }];
    let free = [Limit {
        count: 10,
        window: Duration::from_nanos(1),
    }];
    let table = one_set();
    for caller in 0..WAYS {
        assert_eq!(
            spend(&table, 0, &key(caller), 0, &limits, 1),
            Decision::Allowed
        );
    }
    assert_eq!(spend(&table, 0, b"cheap", 0, &free, 1), Decision::Allowed);
    assert_eq!(table.tats(0, b"cheap"), None);
    assert_eq!(table.evictions(), 0);
}
