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
        || now,
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

/// Every budget from about 5.6 KB up fits and fills at least 95% with slots.
/// From about 485 KiB up, which covers every budget the configuration
/// accepts, the table has 1024 shards.
#[xmtp_common::test]
fn the_layout_fills_the_budget() {
    const FULL_SHARDS_FROM: usize = 496_640;
    let set_bytes = size_of::<Set>();
    let shard_bytes = ParkingTable::SHARD_BYTES;
    let fits = |budget: usize| {
        let (shards, sets) = layout(budget, shard_bytes);
        assert!(shards >= 1 && sets >= shards, "{budget}");
        assert!(
            shards * shard_bytes + sets * set_bytes <= budget,
            "{budget}"
        );
        assert!(
            sets * WAYS * 100 >= budget / size_of::<Slot>() * 95,
            "{budget}"
        );
        shards
    };
    let swept = (5600..2 * MIB).step_by(997);
    for budget in swept.chain([5600, FULL_SHARDS_FROM - 1, 286_700, 400 << 10]) {
        let shards = fits(budget);
        assert_eq!(shards == MAX_SHARDS, budget >= FULL_SHARDS_FROM, "{budget}");
    }
    for budget in [FULL_SHARDS_FROM, MIB, 3 * MIB + 17, 100 * MIB, 65536 * MIB] {
        assert_eq!(fits(budget), MAX_SHARDS, "{budget}");
    }
    // A budget too small for one set and its shard still gets one.
    assert_eq!(layout(0, shard_bytes), (1, 1));
}

/// One index over all sets: when shards differ in length, a set in a short
/// shard takes no more hashes than one in a long shard.
#[xmtp_common::test]
fn every_set_takes_an_equal_share_of_hashes() {
    const KEYS: usize = 30_000;
    let table = ParkingTable::with_layout(2, 3);
    let lens: Vec<usize> = table
        .shards
        .iter()
        .map(|shard| shard.with(|sets| sets.len()))
        .collect();
    let mut counts = std::collections::BTreeMap::new();
    for caller in 0..KEYS {
        let (shard, set) = table.locate(table.hash(0, &key(caller)));
        assert!(set < lens[shard]);
        *counts.entry((shard, set)).or_insert(0usize) += 1;
    }
    assert_eq!(counts.len(), 3);
    // 10,000 each, give or take 82.
    for count in counts.values() {
        assert!((9_000..=11_000).contains(count), "{counts:?}");
    }
}

/// Keys spread over every set: a 1 MiB table half full of callers with spend
/// left forgets about 1% of them, not most.
#[xmtp_common::test]
fn a_half_full_table_rarely_forgets_a_caller() {
    let table = ParkingTable::new(MIB);
    let limits = [Limit {
        count: 1,
        window: Duration::from_secs(3600),
    }];
    let keys = table.slots() / 2;
    for caller in 0..keys {
        assert_eq!(
            spend(&table, 0, &key(caller), 0, &limits, 1),
            Decision::Allowed
        );
    }
    assert!(
        table.evictions() * 50 < keys as u64,
        "{} of {keys} forgotten",
        table.evictions()
    );
}

/// Three threads spending at once through the `parking_lot` shard lock admit
/// what a serial run admits and leave no partial charge: the hour limit denies
/// the third spend after the minute limit admitted it.
#[xmtp_common::test]
fn concurrent_spends_through_the_shard_lock_admit_exactly() {
    use std::sync::atomic::AtomicUsize;
    const ROUNDS: usize = 10_000;
    const THREADS: usize = 3;
    let limits = [
        Limit {
            count: 3,
            window: Duration::from_secs(60),
        },
        Limit {
            count: 2,
            window: Duration::from_secs(3600),
        },
    ];
    let tables: Vec<ParkingTable> = (0..ROUNDS).map(|_| one_set()).collect();
    let arrived = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| {
                for (round, table) in tables.iter().enumerate() {
                    // A spinning start line, so the threads spend within
                    // moments of one another.
                    arrived.fetch_add(1, Ordering::SeqCst);
                    while arrived.load(Ordering::SeqCst) < THREADS * (round + 1) {
                        std::hint::spin_loop();
                    }
                    spend(table, 0, b"caller", 0, &limits, 1);
                }
            });
        }
    });
    for table in &tables {
        assert_eq!(
            table.tats(0, b"caller"),
            Some([40 * SECOND, 3600 * SECOND, 0])
        );
    }
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
/// Until the set is full, no caller is forgotten.
// verifies: RATE-052
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
/// and spent the most units, and every caller with spend left is kept.
// verifies: RATE-052
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
