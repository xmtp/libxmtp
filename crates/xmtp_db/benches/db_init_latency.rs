//! Measures how libxmtp's fresh-DB creation (the ~63 Diesel migrations) scales
//! with per-operation disk latency, using the `latency_vfs` shim to inject a
//! configurable delay on each SQLite `xSync` / `xWrite`.
//!
//! Motivation: on network-attached storage (e.g. AWS EFS) every SQLite op pays
//! a round-trip, so `POST /v1/agents` — which creates a fresh encrypted store —
//! is dominated by migration I/O. This bench reproduces that curve on demand so
//! we can quantify the cost and the payoff of avoiding per-account migrations.
//!
//! Run with:
//!   cargo bench -p xmtp_db --features bench --bench db_init_latency
//!
//! Two scenarios are swept over {0, 1, 5, 10} ms per op:
//! * `fsync_latency` — delay on `xSync` only (WAL defers fsync to checkpoint, so
//!   this isolates the true fsync cost)
//! * `write_sync_latency` — delay on `xWrite` + `xSync` (closer to the EFS per-op
//!   RTT model, where every write is a round-trip)

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use xmtp_db::{EncryptedMessageStore, NativeDb, latency_vfs};

/// Latencies (ms per op) to sweep.
const LATENCIES_MS: &[u64] = &[0, 1, 5, 10];

/// Base directory for the bench DB files. Defaults to the OS temp dir; override
/// with `XMTP_BENCH_DIR` to benchmark a real mount (e.g. an Archil/EFS volume)
/// with zero injected latency, so the numbers reflect that disk's true latency.
fn bench_base_dir() -> std::path::PathBuf {
    match std::env::var_os("XMTP_BENCH_DIR") {
        Some(d) => std::path::PathBuf::from(d),
        None => std::env::temp_dir(),
    }
}

/// Returns a fresh, unique DB path under the bench base dir and best-effort
/// removes any stale files at that path.
fn fresh_db_path() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = bench_base_dir().join(format!("xmtp-latbench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp bench dir");
    let path = dir.join(format!("inbox-{n}.db3"));
    for suffix in ["", "-wal", "-shm", "-journal", ".sqlcipher_salt"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
    path.to_string_lossy().into_owned()
}

/// The unique filename component of `path`, used to scope the latency VFS
/// (matches the DB file and its `-wal`/`-shm`/`-journal` siblings, and survives
/// SQLite's path canonicalization).
fn scope_token(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .expect("db path has a filename")
        .to_string_lossy()
        .into_owned()
}

/// Build a fresh persistent, encrypted store at `path`. This is the operation
/// under test: `EncryptedMessageStore::new` runs `init()`, which applies all
/// pending migrations. Single-connection mode mirrors herald-lite
/// (`useSingleConnection: true`).
fn create_store(path: &str) -> EncryptedMessageStore<NativeDb> {
    let db = NativeDb::builder()
        .persistent(path)
        .key([0u8; 32])
        .single_connection()
        .build()
        .expect("build native db");
    EncryptedMessageStore::new(db).expect("create encrypted store")
}

/// Prints the SQLite I/O op counts for a single fresh-store creation, so the
/// latency curves can be read as roughly (op-count x per-op latency).
fn report_op_counts() {
    latency_vfs::set_delays(Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let path = fresh_db_path();
    latency_vfs::set_scope_prefix(Some(scope_token(&path)));
    latency_vfs::reset_counts();
    let store = create_store(&path);
    let counts = latency_vfs::counts();
    drop(store);
    latency_vfs::set_scope_prefix(None);
    println!(
        "\n[db_init_latency] fresh-store I/O ops: open={} (total_open={}) write={} sync={} read={}\n",
        counts.open, counts.total_open, counts.write, counts.sync, counts.read
    );
}

/// Benchmark one scenario across the latency sweep. `delays` maps a per-op
/// latency to the (sync, write, read) delays to apply.
fn sweep(
    c: &mut Criterion,
    group_name: &str,
    delays: impl Fn(Duration) -> (Duration, Duration, Duration),
) {
    let mut group = c.benchmark_group(group_name);
    group.sample_size(10);
    for &ms in LATENCIES_MS {
        let (sync, write, read) = delays(Duration::from_millis(ms));
        group.bench_with_input(BenchmarkId::from_parameter(ms), &ms, |b, _| {
            b.iter_batched(
                || {
                    // Fresh DB per iteration; scope the delay to just its files.
                    let path = fresh_db_path();
                    latency_vfs::set_scope_prefix(Some(scope_token(&path)));
                    latency_vfs::set_delays(sync, write, read);
                    path
                },
                |path| create_store(&path),
                BatchSize::PerIteration,
            );
        });
    }
    latency_vfs::set_scope_prefix(None);
    latency_vfs::set_delays(Duration::ZERO, Duration::ZERO, Duration::ZERO);
    group.finish();
}

fn bench_db_init_latency(c: &mut Criterion) {
    latency_vfs::register().expect("register latency vfs");
    report_op_counts();

    // fsync-only: WAL defers fsync to checkpoint, so this isolates fsync cost.
    sweep(c, "fsync_latency", |d| (d, Duration::ZERO, Duration::ZERO));
    // write + sync: closer to the EFS per-op round-trip model.
    sweep(c, "write_sync_latency", |d| (d, d, Duration::ZERO));
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = bench_db_init_latency
);
criterion_main!(benches);
