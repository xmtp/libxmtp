//! Cryptography configuration primitives

use openmls::versions::ProtocolVersion;
use openmls_traits::types::Ciphersuite;

pub const MLS_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::Mls10;

pub const CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519;

pub const POST_QUANTUM_CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_256_XWING_CHACHA20POLY1305_SHA256_Ed25519;

pub const ED25519_KEY_LENGTH: usize = 32;
