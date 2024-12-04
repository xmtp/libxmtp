// //! Benchmarking for syncing functions
// use crate::tracing::Instrument;
// use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
// use tokio::runtime::{Builder, Runtime};
// use xmtp_mls::utils::bench::{bench_async_setup, BENCH_ROOT_SPAN};
// use xmtp_mls::utils::bench::{clients, init_logging};
//
// #[macro_use]
// extern crate tracing;
//
// fn setup() -> Runtime {
//     Builder::new_multi_thread()
//         .enable_time()
//         .enable_io()
//         .thread_name("xmtp-bencher")
//         .build()
//         .unwrap()
// }
//
// fn start_sync_worker(c: &mut Criterion) {
//     init_logging();
//
//     let runtime = setup();
//     let mut benchmark_group = c.benchmark_group("start_sync_worker");
//     benchmark_group.sample_size(10);
//     benchmark_group.bench_function("start_sync_worker", |b| {
//         let span = trace_span!(BENCH_ROOT_SPAN);
//         b.to_async(&runtime).iter_batched(
//             || {
//                 bench_async_setup(|| async {
//                     let client = clients::new_client(true).await;
//                     // set history sync URL
//                     (client, span.clone())
//                 })
//             },
//             |(client, span)| async move { client.start_sync_worker().instrument(span) },
//             BatchSize::SmallInput,
//         )
//     });
//
//     benchmark_group.finish();
// }

// criterion_group!(
//     name = sync;
//     config = Criterion::default().sample_size(10);
//     targets = start_sync_worker
// );
// criterion_main!(sync);
