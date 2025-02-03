//!  NOTE:
// `MAX_DB_POOL_SIZE` in `configuration.rs` must be set to `10`
// in order for these benchmarks to successfully run & generate a report.
// (file descriptor issue)

use crate::tracing::Instrument;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use tokio::runtime::{Builder, Runtime};
use xmtp_common::{
    bench::{bench_async_setup, BENCH_ROOT_SPAN},
    tmp_path,
};
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
    benchmark_group.bench_function("create_ffi_client", |b| {
        let span = trace_span!(BENCH_ROOT_SPAN);
        b.to_async(&runtime).iter_batched(
            || {
                bench_async_setup(|| async {
                    let wallet = xmtp_cryptography::utils::generate_local_wallet();
                    let nonce = 1;
                    let inbox_id = generate_inbox_id(wallet.get_address(), nonce).unwrap();
                    let path = tmp_path();
                    let (url, is_secure) = network_url();
                    let api = xmtpv3::mls::connect_to_backend(url, is_secure)
                        .await
                        .unwrap();
                    (
                        api,
                        inbox_id,
                        wallet.get_address(),
                        nonce,
                        path,
                        span.clone(),
                    )
                })
            },
            |(api, inbox_id, address, nonce, path, span)| async move {
                xmtpv3::mls::create_client(
                    api,
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
    let (url, is_secure) = network_url();
    let api = runtime.block_on(async {
        let api = xmtpv3::mls::connect_to_backend(url.clone(), is_secure)
            .await
            .unwrap();
        xmtpv3::mls::create_client(
            api.clone(),
            Some(path.clone()),
            Some(vec![0u8; 32]),
            &inbox_id.clone(),
            address.clone(),
            nonce,
            None,
            Some(HISTORY_SYNC_URL.to_string()),
        )
        .await
        .unwrap();
        api
    });

    // benchmark_group.sample_size(10);
    benchmark_group.bench_function("cached_create_ffi_client", |b| {
        let span = trace_span!(BENCH_ROOT_SPAN);
        b.to_async(&runtime).iter_batched(
            || {
                (
                    api.clone(),
                    inbox_id.clone(),
                    address.clone(),
                    nonce,
                    path.clone(),
                    HISTORY_SYNC_URL.to_string(),
                    span.clone(),
                )
            },
            |(api, inbox_id, address, nonce, path, history_sync, span)| async move {
                xmtpv3::mls::create_client(
                    api,
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
