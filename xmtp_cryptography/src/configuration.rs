//! Cryptography configuration primitives

use openmls_traits::types::Ciphersuite;

pub const CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519;
