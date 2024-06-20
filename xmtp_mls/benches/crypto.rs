use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use openmls_rust_crypto::RustCrypto;
use openmls_traits::{crypto::OpenMlsCrypto, random::OpenMlsRand};
use rand::{rngs::OsRng, RngCore};
use xmtp_mls::configuration::CIPHERSUITE;
use xmtp_mls::utils::bench::re_export::encrypt_welcome;

fn bench_encrypt_welcome(c: &mut Criterion) {
    let sizes = [
        16,
        32,
        64,
        128,
        256,
        512,
        1024,
        2048,
        4096,
        8192,
        16384,
        32768,
        65536,
        131_072,
        262_144,
        524_288,
        1_048_576,
        2_097_152,
        4_194_304,
        8_388_608,
        16_777_216,
        33_554_432,
        67_108_864,
        134_217_728,
    ];

    let mut benchmark_group = c.benchmark_group("encrypt_welcome");
    benchmark_group.sample_size(10);

    for size in sizes.iter() {
        benchmark_group.throughput(Throughput::Bytes(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || {
                    let crypto = RustCrypto::default();
                    let ikm = crypto.random_vec(CIPHERSUITE.hash_length()).unwrap();
                    let keypair = crypto
                        .derive_hpke_keypair(CIPHERSUITE.hpke_config(), ikm.as_slice())
                        .unwrap();

                    let mut payload = vec![0; size];
                    OsRng.fill_bytes(payload.as_mut_slice());
                    (payload, keypair.public)
                },
                |(payload, key)| encrypt_welcome(&payload, &key),
                BatchSize::SmallInput,
            )
        });
    }

    benchmark_group.finish();
}

criterion_group!(
    name = crypto;
    config = Criterion::default().sample_size(10);
    targets = bench_encrypt_welcome
);
criterion_main!(crypto);
