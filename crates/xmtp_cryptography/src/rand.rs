use rand::distr::Alphanumeric;
use rand::distr::SampleString;
use rand::{CryptoRng, Rng, RngExt, SeedableRng};
use rand_chacha::ChaCha20Rng;

use crate::Secret;

pub fn rng() -> impl CryptoRng + RngExt {
    ChaCha20Rng::try_from_rng(&mut rand::rngs::SysRng).unwrap()
}

pub fn seeded_rng(seed: u64) -> impl CryptoRng + RngExt {
    ChaCha20Rng::seed_from_u64(seed)
}

pub fn rand_string<const N: usize>() -> String {
    Alphanumeric.sample_string(&mut rng(), N)
}

pub fn rand_array<const N: usize>() -> [u8; N] {
    let mut buffer = [0u8; N];
    rng().fill_bytes(&mut buffer);
    buffer
}

pub fn rand_vec<const N: usize>() -> Vec<u8> {
    rand_array::<N>().to_vec()
}

pub fn rand_secret<const N: usize>() -> Secret {
    Secret::new(rand_vec::<N>())
}
