use crate::tracing::Instrument;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use tokio::runtime::{Builder, Runtime};
use xmtp_common::bench::{self, bench_async_setup, BENCH_ROOT_SPAN};
use xmtp_id::{associations::builder::SignatureRequest, InboxOwner};
use xmtp_mls::utils::bench::{clients, BenchClient};

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

async fn ecdsa_signature(client: &BenchClient, owner: impl InboxOwner) -> SignatureRequest {
    let mut signature_request = client.context.signature_request().unwrap();
    let signature_text = signature_request.signature_text();
    let unverified_signature = owner.sign(&signature_text).unwrap();
    signature_request
        .add_signature(unverified_signature, client.scw_verifier())
        .await
        .unwrap();

    signature_request
}

fn register_identity_eoa(c: &mut Criterion) {
    bench::logger();

    let runtime = setup();

    let mut benchmark_group = c.benchmark_group("register_identity");
    benchmark_group.sample_size(10);
    benchmark_group.bench_function("register_identity_eoa", |b| {
        let span = trace_span!(BENCH_ROOT_SPAN);
        b.to_async(&runtime).iter_batched(
            || {
                bench_async_setup(|| async {
                    let (client, wallet) = clients::new_unregistered_client(false).await;
                    let signature_request = ecdsa_signature(&client, wallet).await;

                    (client, signature_request, span.clone())
                })
            },
            |(client, request, span)| async move {
                client
                    .register_identity(request)
                    .instrument(span)
                    .await
                    .unwrap()
            },
            BatchSize::SmallInput,
        )
    });

    benchmark_group.finish();
}

criterion_group!(
    name = identity;
    config = Criterion::default().sample_size(10);
    targets = register_identity_eoa
);
criterion_main!(identity);
