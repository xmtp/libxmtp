use openmls::versions::ProtocolVersion;
use openmls_traits::types::Ciphersuite;

// TODO confirm ciphersuite choice
pub const CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519;

pub const MLS_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::Mls10;
