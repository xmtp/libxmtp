//! Benchmarks for message finding operations with shared setup
//!
//! This version shares the expensive setup between benchmark samples to reduce total runtime.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::Rng;
use std::{hint::black_box, sync::Arc, time::Duration};
use tokio::runtime::{Builder, Runtime};
use tracing::{Instrument, trace_span};
use xmtp_common::bench::{self, BENCH_ROOT_SPAN};
use xmtp_db::prelude::QueryConsentRecord;
use xmtp_mls::utils::bench::{ConsentBenchSetup, create_dm_with_consent, new_client};

pub const CONSENT_COUNTS: &[usize] = &[10, 100, 1000, 10000, 50000];
pub const SAMPLE_SIZE: usize = 10;

fn setup_runtime() -> Runtime {
    Builder::new_multi_thread()
        .enable_time()
        .enable_io()
        .thread_name("xmtp-consent-bencher")
        .build()
        .unwrap()
}

/// Creates n dm groups and associated consent records
async fn setup_benchmark(total_consents: usize) -> Arc<ConsentBenchSetup> {
    let client = new_client().await;
    create_dm_with_consent(client, total_consents).await
}

fn bench_find_consent_by_dm_id(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("find_consent_by_dm_id");
    benchmark_group.sample_size(SAMPLE_SIZE);
    benchmark_group.measurement_time(Duration::from_secs(10));
    benchmark_group.warm_up_time(Duration::from_secs(1));

    let runtime = Arc::new(setup_runtime());

    for &total_consents in CONSENT_COUNTS.iter() {
        benchmark_group.throughput(Throughput::Elements(10)); // Limit of 10

        // Setup once per CONSENT_COUNT - completely outside the benchmark
        let setup = runtime.block_on(setup_benchmark(total_consents));
        let runtime_clone = runtime.clone();

        benchmark_group.bench_function(
            BenchmarkId::new("find_consent_by_dm_id", total_consents),
            move |b| {
                let span = trace_span!(BENCH_ROOT_SPAN, total_consents);
                let setup = setup.clone();
                let runtime = runtime_clone.clone();

                b.iter(|| {
                    runtime.block_on(
                        async {
                            let dm_id =
                                &setup.dm_ids[rand::thread_rng().gen_range(0..total_consents)];
                            let consent = setup.client.db().find_consent_by_dm_id(dm_id).unwrap();

                            assert_eq!(
                                consent.len(),
                                1,
                                "Expected exactly 1 consent, got {}",
                                consent.len()
                            );
                            black_box(consent);
                        }
                        .instrument(span.clone()),
                    )
                });
            },
        );
    }

    benchmark_group.finish();
}

criterion_group!(
    name = consent;
    config = Criterion::default()
        .sample_size(SAMPLE_SIZE)
        .measurement_time(Duration::from_secs(30))
        .warm_up_time(Duration::from_secs(3));
    targets = bench_find_consent_by_dm_id

);
criterion_main!(consent);
