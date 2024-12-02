use crate::tracing::Instrument;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use ethers::signers::LocalWallet;
use tokio::runtime::{Builder, Runtime};
use xmtp_id::{
    associations::{
        builder::SignatureRequest,
        generate_inbox_id,
        unverified::{UnverifiedRecoverableEcdsaSignature, UnverifiedSignature},
    },
    InboxOwner,
};
use xmtp_mls::utils::{bench::init_logging, test::TestClient as TestApiClient};
use xmtp_mls::{
    client::Client,
    identity::IdentityStrategy,
    utils::bench::{bench_async_setup, BENCH_ROOT_SPAN},
};
use xmtp_proto::api_client::XmtpTestClient;

type BenchClient = Client<TestApiClient>;

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

async fn new_client() -> (BenchClient, LocalWallet) {
    let nonce = 1;
    let wallet = xmtp_cryptography::utils::generate_local_wallet();
    let inbox_id = generate_inbox_id(&wallet.get_address(), &nonce).unwrap();

    let dev = std::env::var("DEV_GRPC");
    let is_dev_network = matches!(dev, Ok(d) if d == "true" || d == "1");

    let api_client = if is_dev_network {
        tracing::info!("Using Dev GRPC");
        <TestApiClient as XmtpTestClient>::create_dev().await
    } else {
        tracing::info!("Using Local GRPC");
        <TestApiClient as XmtpTestClient>::create_local().await
    };

    let client = BenchClient::builder(IdentityStrategy::new(
        inbox_id,
        wallet.get_address(),
        nonce,
        None,
    ));

    let client = client
        .temp_store()
        .await
        .api_client(api_client)
        .build()
        .await
        .unwrap();

    (client, wallet)
}

async fn ecdsa_signature(client: &BenchClient, owner: impl InboxOwner) -> SignatureRequest {
    let mut signature_request = client.context().signature_request().unwrap();
    let signature_text = signature_request.signature_text();
    let unverified_signature = UnverifiedSignature::RecoverableEcdsa(
        UnverifiedRecoverableEcdsaSignature::new(owner.sign(&signature_text).unwrap().into()),
    );
    signature_request
        .add_signature(unverified_signature, client.scw_verifier())
        .await
        .unwrap();

    signature_request
}

fn register_identity_eoa(c: &mut Criterion) {
    init_logging();

    let runtime = setup();

    let mut benchmark_group = c.benchmark_group("register_identity");
    benchmark_group.sample_size(10);
    benchmark_group.bench_function("register_identity_eoa", |b| {
        let span = trace_span!(BENCH_ROOT_SPAN);
        b.to_async(&runtime).iter_batched(
            || {
                bench_async_setup(|| async {
                    let (client, wallet) = new_client().await;
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
