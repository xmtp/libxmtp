//!  NOTE:
// `MAX_DB_POOL_SIZE` in `configuration.rs` must be set to `10`
// in order for these benchmarks to succesfully run & generate a report.
// (file descriptor issue)

use crate::tracing::Instrument;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use tokio::runtime::{Builder, Runtime};
use xmtp_common::{bench::BENCH_ROOT_SPAN, tmp_path};
use xmtp_id::InboxOwner;
use xmtp_mls::utils::test::HISTORY_SYNC_URL;
use xmtpv3::generate_inbox_id;

#[macro_use]
extern crate tracing;

fn setup() -> Runtime {
    Builder::new_multi_thread()
        .enable_time()
        .enable_io()
        .thread_name("xmtp-bencher")
        .build()
        .unwrap()
}

fn network_url() -> (String, bool) {
    let dev = std::env::var("DEV_GRPC");
    let is_dev_network = matches!(dev, Ok(d) if d == "true" || d == "1");

    if is_dev_network {
        (xmtp_api_grpc::DEV_ADDRESS.to_string(), true)
    } else {
        (xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(), false)
    }
}

fn create_ffi_client(c: &mut Criterion) {
    xmtp_common::bench::logger();

    let runtime = setup();

    let _ = fdlimit::raise_fd_limit();
    let mut benchmark_group = c.benchmark_group("create_client");

    // benchmark_group.sample_size(10);
    benchmark_group.sampling_mode(criterion::SamplingMode::Flat);
    benchmark_group.bench_function("create_ffi_client", |b| {
        let span = trace_span!(BENCH_ROOT_SPAN);
        b.to_async(&runtime).iter_batched(
            || {
                let wallet = xmtp_cryptography::utils::generate_local_wallet();
                let nonce = 1;
                let inbox_id = generate_inbox_id(wallet.get_address(), nonce).unwrap();
                let path = tmp_path();
                let (network, is_secure) = network_url();
                (
                    inbox_id,
                    wallet.get_address(),
                    nonce,
                    path,
                    network,
                    is_secure,
                    span.clone(),
                )
            },
            |(inbox_id, address, nonce, path, network, is_secure, span)| async move {
                xmtpv3::mls::create_client(
                    network,
                    is_secure,
                    Some(path),
                    Some(vec![0u8; 32]),
                    &inbox_id,
                    address,
                    nonce,
                    None,
                    Some(HISTORY_SYNC_URL.to_string()),
                )
                .instrument(span)
                .await
                .unwrap();
            },
            BatchSize::SmallInput,
        )
    });

    benchmark_group.finish();
}

fn cached_create_ffi_client(c: &mut Criterion) {
    xmtp_common::bench::logger();

    let runtime = setup();

    let _ = fdlimit::raise_fd_limit();
    let mut benchmark_group = c.benchmark_group("create_client_from_cached");
    let wallet = xmtp_cryptography::utils::generate_local_wallet();
    let nonce = 1;
    let inbox_id = generate_inbox_id(wallet.get_address(), nonce).unwrap();
    let address = wallet.get_address();
    let path = tmp_path();

    // benchmark_group.sample_size(10);
    benchmark_group.sampling_mode(criterion::SamplingMode::Flat);
    benchmark_group.bench_function("cached_create_ffi_client", |b| {
        let span = trace_span!(BENCH_ROOT_SPAN);
        b.to_async(&runtime).iter_batched(
            || {
                let (network, is_secure) = network_url();
                (
                    inbox_id.clone(),
                    address.clone(),
                    nonce,
                    path.clone(),
                    HISTORY_SYNC_URL.to_string(),
                    network,
                    is_secure,
                    span.clone(),
                )
            },
            |(inbox_id, address, nonce, path, history_sync, network, is_secure, span)| async move {
                xmtpv3::mls::create_client(
                    network,
                    is_secure,
                    Some(path),
                    Some(vec![0u8; 32]),
                    &inbox_id,
                    address,
                    nonce,
                    None,
                    Some(history_sync),
                )
                .instrument(span)
                .await
                .unwrap();
            },
            BatchSize::SmallInput,
        )
    });

    benchmark_group.finish();
}

criterion_group!(
    name = create_client;
    config = Criterion::default().sample_size(10);
    targets = create_ffi_client, cached_create_ffi_client
);
criterion_main!(create_client);
