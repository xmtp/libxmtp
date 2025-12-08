//!  NOTE:
// `MAX_DB_POOL_SIZE` in `configuration.rs` must be set to `10`
// in order for these benchmarks to successfully run & generate a report.
// (file descriptor issue)

use crate::tracing::Instrument;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use tokio::runtime::{Builder, Runtime};
use xmtp_common::{
    bench::{BENCH_ROOT_SPAN, bench_async_setup},
    tmp_path,
};
use xmtp_id::associations::test_utils::WalletTestExt;
use xmtpv3::identity::FfiIdentifier;

#[macro_use]
extern crate tracing;

const HISTORY_SYNC_URL: &str = xmtp_configuration::DeviceSyncUrls::LOCAL_ADDRESS;

fn setup() -> Runtime {
    Builder::new_multi_thread()
        .enable_time()
        .enable_io()
        .thread_name("xmtp-bencher")
        .build()
        .unwrap()
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
                    let ident = wallet.identifier();
                    let nonce = 1;
                    let inbox_id = ident.inbox_id(nonce).unwrap();
                    let path = tmp_path();
                    let api = xmtpv3::test_utils::connect_to_backend_test().await;
                    let sync_api = xmtpv3::test_utils::connect_to_backend_test().await;
                    (
                        api,
                        sync_api,
                        inbox_id,
                        wallet.identifier(),
                        nonce,
                        path,
                        span.clone(),
                    )
                })
            },
            |(api, sync_api, inbox_id, ident, nonce, path, span)| async move {
                let ffi_ident: FfiIdentifier = ident.into();
                xmtpv3::mls::create_client(
                    api,
                    sync_api,
                    Some(path),
                    Some(vec![0u8; 32]),
                    &inbox_id,
                    ffi_ident,
                    nonce,
                    None,
                    Some(HISTORY_SYNC_URL.to_string()),
                    None,
                    None,
                    None,
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
    let ident = wallet.identifier();
    let nonce = 1;
    let inbox_id = ident.inbox_id(nonce).unwrap();
    let ffi_ident: FfiIdentifier = ident.into();
    let address = wallet.identifier();
    let path = tmp_path();
    let api = runtime.block_on(async {
        let api = xmtpv3::test_utils::connect_to_backend_test().await;
        xmtpv3::mls::create_client(
            api.clone(),
            api.clone(),
            Some(path.clone()),
            Some(vec![0u8; 32]),
            &inbox_id.clone(),
            ffi_ident,
            nonce,
            None,
            Some(HISTORY_SYNC_URL.to_string()),
            None,
            None,
            None,
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
            |(api, inbox_id, ident, nonce, path, history_sync, span)| async move {
                let ffi_ident: FfiIdentifier = ident.into();
                xmtpv3::mls::create_client(
                    api.clone(),
                    api,
                    Some(path),
                    Some(vec![0u8; 32]),
                    &inbox_id,
                    ffi_ident,
                    nonce,
                    None,
                    Some(history_sync),
                    None,
                    None,
                    None,
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
