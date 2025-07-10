#![recursion_limit = "256"]
//! Benchmarks for group limit
//! using `RUST_LOG=trace` will additionally output a `tracing.folded` file, which
//! may be used to generate a flamegraph of execution from tracing logs.
use criterion::BatchSize;
use criterion::BenchmarkId;
use criterion::Throughput;
use criterion::{criterion_group, criterion_main, Criterion};
use parking_lot::Mutex;
use rand::seq::SliceRandom;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::runtime::{Builder, Runtime};
use tracing::{trace_span, Instrument};
use xmtp_common::bench::LOGGER;
use xmtp_common::bench::{self, bench_async_setup, BENCH_ROOT_SPAN};
use xmtp_cryptography::utils::rng;
use xmtp_id::InboxOwner;
use xmtp_mls::groups::MlsGroup;
use xmtp_mls::utils::register_client;
use xmtp_mls::{
    builder::SyncWorkerMode,
    identity::IdentityStrategy,
    utils::{test::TestClient as TestApiClient, TestClient},
    Client,
};
use xmtp_proto::api_client::ApiBuilder;
use xmtp_proto::prelude::XmtpTestClient;

pub const SAMPLES: [usize; 6] = [5, 25, 50, 100, 200, 300];
pub const SAMPLE_SIZE: usize = 10;

type BenchGroup = MlsGroup<TestApiClient, xmtp_db::DefaultStore>;

async fn create_client(events: bool, device_sync: bool) -> Client<TestApiClient> {
    let wallet = xmtp_cryptography::utils::generate_local_wallet();

    // use dev network if `DEV_GRPC` is set
    let dev = std::env::var("DEV_GRPC");
    let is_dev_network = matches!(dev, Ok(d) if d == "true" || d == "1");
    let api_client = if is_dev_network {
        tracing::info!("Using Dev GRPC");
        <TestClient as XmtpTestClient>::create_dev()
    } else {
        <TestClient as XmtpTestClient>::create_local()
    };
    let api_client = api_client.build().await.unwrap();
    let nonce = 1;
    let identifier = wallet.get_identifier().unwrap();
    let inbox_id = identifier.inbox_id(nonce).unwrap();
    let client = Client::builder(IdentityStrategy::new(inbox_id, identifier, nonce, None));

    let mut client = client
        .temp_store()
        .await
        .api_client(api_client)
        .with_remote_verifier()
        .unwrap();

    if !events {
        client = client.with_disable_events(Some(true))
    };
    if !device_sync {
        client = client.device_sync_worker_mode(SyncWorkerMode::Disabled)
    };

    let client = client.build().await.unwrap();
    register_client(&client, &wallet).await;
    client
}

fn setup() -> (Client<TestApiClient>, Runtime) {
    let runtime = Builder::new_multi_thread()
        .enable_time()
        .enable_io()
        .thread_name("xmtp-bencher")
        .build()
        .unwrap();

    let client = runtime.block_on(async { create_client(false, false).await });

    (client, runtime)
}

async fn add_to_groups(
    client: &Client<TestApiClient>,
    num_groups: usize,
    ids: &[String],
) -> (Vec<BenchGroup>, Vec<String>) {
    let mut groups = vec![];
    let mut ids = ids.to_vec();
    for _ in 0..num_groups {
        let transient_identity = create_client(false, false).await;
        ids.push(transient_identity.inbox_id().to_string());
        let mut invitees = vec![client.inbox_id().to_string()];
        invitees.extend(ids.clone());
        let group = transient_identity
            .create_group_with_inbox_ids(&invitees, None, None)
            .await
            .unwrap();
        group.sync().await.unwrap();
        groups.push(group);
    }
    (groups, ids)
}

async fn add_messages(groups: &[BenchGroup], messages: usize) {
    let mut rand = rng();
    for _ in 0..messages {
        let group = groups.choose(&mut rand).unwrap();
        group.send_message(b"test").await.unwrap();
        group.sync().await.unwrap();
    }
}

fn sync_10_groups_many_messages(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("sync_10_groups_many_messages");
    benchmark_group.sample_size(SAMPLE_SIZE);

    let (client, runtime) = setup();
    let (groups, ids) = runtime.block_on(async {
        let (groups, ids) = add_to_groups(&client, 10, &[]).await;
        client.sync_welcomes().await.unwrap();
        (groups, ids)
    });

    for size in SAMPLES.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let span = trace_span!(BENCH_ROOT_SPAN, size);
            b.to_async(&runtime).iter_batched(
                || {
                    bench_async_setup(|| async {
                        add_messages(&groups, size).await;
                        (&client, span.clone())
                    })
                },
                |(client, span)| async move {
                    client
                        .sync_all_welcomes_and_groups(None)
                        .instrument(span)
                        .await
                        .unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }

    let guard = unsafe { LOGGER.get_unchecked() };
    let _ = guard.flush();

    benchmark_group.finish();
}

fn sync_10_messages_many_groups(c: &mut Criterion) {
    let _ = fdlimit::raise_fd_limit();
    bench::logger();
    let mut benchmark_group = c.benchmark_group("sync_10_messages_many_groups");
    benchmark_group.sample_size(SAMPLE_SIZE);

    let (client, runtime) = setup();
    let groups = Arc::new(Mutex::new(vec![]));

    let currently_added_len = AtomicUsize::new(0);
    for size in SAMPLES.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let span = trace_span!(BENCH_ROOT_SPAN, size);
            b.to_async(&runtime).iter_batched(
                || {
                    bench_async_setup(|| async {
                        let to_add = size - currently_added_len.load(Ordering::SeqCst);
                        let (grps, ids) = add_to_groups(&client, to_add, &[]).await;
                        let groups = {
                            let mut groups = groups.lock();
                            groups.extend(grps);
                            groups.clone()
                        };
                        add_messages(&groups, size).await;
                        currently_added_len.fetch_add(to_add, Ordering::SeqCst);
                        (&client, span.clone())
                    })
                },
                |(client, span)| async move {
                    client
                        .sync_all_welcomes_and_groups(None)
                        .instrument(span)
                        .await
                        .unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    let guard = unsafe { LOGGER.get_unchecked() };
    let _ = guard.flush();

    benchmark_group.finish();
}

criterion_group!(
    name = sync_all;
    config = Criterion::default().sample_size(10);
    targets = sync_10_groups_many_messages, sync_10_messages_many_groups
);
criterion_main!(sync_all);
