use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use openmls_rust_crypto::RustCrypto;
use openmls_traits::{crypto::OpenMlsCrypto, random::OpenMlsRand};
use rand::{TryRng, rngs::SysRng};
use xmtp_configuration::{CIPHERSUITE, POST_QUANTUM_CIPHERSUITE};
use xmtp_mls::utils::bench::re_export::{WrapperAlgorithm, wrap_welcome};

const BENCH_SIZES: [usize; 24] = [
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

fn bench_encrypt_welcome_curve25519(c: &mut Criterion) {
    let mut benchmark_group = c.benchmark_group("wrap_welcome_curve25519");
    benchmark_group.sample_size(10);

    for size in BENCH_SIZES.iter() {
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
                    SysRng.try_fill_bytes(payload.as_mut_slice()).unwrap();
                    (payload, keypair.public)
                },
                |(payload, key)| wrap_welcome(&payload, &[], &key, WrapperAlgorithm::Curve25519),
                BatchSize::SmallInput,
            )
        });
    }

    benchmark_group.finish();
}

fn bench_encrypt_welcome_post_quantum(c: &mut Criterion) {
    let mut benchmark_group = c.benchmark_group("wrap_welcome_post_quantum");
    benchmark_group.sample_size(10);

    for size in BENCH_SIZES.iter() {
        benchmark_group.throughput(Throughput::Bytes(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || {
                    let crypto = openmls_libcrux_crypto::CryptoProvider::new().unwrap();
                    let ikm = crypto
                        .random_vec(POST_QUANTUM_CIPHERSUITE.hash_length())
                        .unwrap();
                    let keypair = crypto
                        .derive_hpke_keypair(POST_QUANTUM_CIPHERSUITE.hpke_config(), ikm.as_slice())
                        .unwrap();
                    let mut payload = vec![0; size];
                    SysRng.try_fill_bytes(payload.as_mut_slice()).unwrap();
                    (payload, keypair.public)
                },
                |(payload, key)| {
                    wrap_welcome(&payload, &[], &key, WrapperAlgorithm::XWingMLKEM768Draft6)
                },
                BatchSize::SmallInput,
            )
        });
    }

    benchmark_group.finish();
}

criterion_group!(
    name = crypto;
    config = Criterion::default().sample_size(10);
    targets = bench_encrypt_welcome_curve25519, bench_encrypt_welcome_post_quantum
);
criterion_main!(crypto);
