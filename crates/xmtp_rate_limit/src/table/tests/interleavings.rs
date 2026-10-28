//! Concurrent spends under every thread schedule shuttle can produce. These run
//! in the default test build, with no feature or `cfg`.

use std::sync::Arc;

use super::*;

type ShuttleTable = Table<shuttle::sync::Mutex<Box<[Set]>>>;

impl ShardLock for shuttle::sync::Mutex<Box<[Set]>> {
    fn new(sets: Box<[Set]>) -> Self {
        shuttle::sync::Mutex::new(sets)
    }

    fn with<R>(&self, f: impl FnOnce(&mut [Set]) -> R) -> R {
        f(&mut self
            .lock()
            .expect("no thread panics while it holds a shard"))
    }
}

const THREADS: usize = 3;
const NOW: u64 = 0;
const CALLER: &[u8] = b"caller";
const CATEGORY: u8 = 3;
/// The minute limit denies the third spend.
const MINUTE_DENIES: [Limit; 2] = [
    Limit {
        count: 2,
        window: Duration::from_secs(60),
    },
    Limit {
        count: 10,
        window: Duration::from_secs(3600),
    },
];
/// The hour limit denies the third spend, after the minute limit admitted it.
const HOUR_DENIES: [Limit; 2] = [
    Limit {
        count: 3,
        window: Duration::from_secs(60),
    },
    Limit {
        count: 2,
        window: Duration::from_secs(3600),
    },
];

fn spend_once<L: ShardLock>(table: &Table<L>, limits: &[Limit]) -> bool {
    let spend = Spend {
        key: CALLER,
        category: CATEGORY,
        limits,
        cost: 1,
    };
    table.spend(NOW, &spend) == Decision::Allowed
}

/// What one thread spending `THREADS` times admits and leaves recorded.
fn serial(limits: &'static [Limit]) -> (usize, Option<Tats>) {
    let table = ParkingTable::with_layout(1, 1);
    let admitted = (0..THREADS).filter(|_| spend_once(&table, limits)).count();
    (admitted, table.tats(CATEGORY, CALLER))
}

#[xmtp_common::test]
fn concurrent_spends_admit_what_a_serial_run_admits() {
    // Two spends of 1 at 0: the minute limit is 2 × 30 s on, the hour 2 × 360 s;
    // or 2 × 20 s and 2 × 1800 s.
    let charged = [
        Some([60 * SECOND, 720 * SECOND, 0]),
        Some([40 * SECOND, 3600 * SECOND, 0]),
    ];
    for (limits, charged) in [&MINUTE_DENIES, &HOUR_DENIES].into_iter().zip(charged) {
        let (admitted, recorded) = serial(limits);
        assert_eq!(admitted, 2);
        // The denied third spend charged neither limit.
        assert_eq!(recorded, charged);
        shuttle::check_dfs(
            move || {
                let table = Arc::new(ShuttleTable::with_layout(1, 1));
                let threads: Vec<_> = (0..THREADS)
                    .map(|_| {
                        let table = Arc::clone(&table);
                        shuttle::thread::spawn(move || spend_once(&table, limits))
                    })
                    .collect();
                let concurrent = threads
                    .into_iter()
                    .map(|thread| thread.join().expect("no spend panics"))
                    .filter(|&allowed| allowed)
                    .count();
                assert_eq!(concurrent, admitted);
                assert_eq!(table.tats(CATEGORY, CALLER), recorded);
            },
            None,
        );
    }
}
