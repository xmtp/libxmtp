use openmls::versions::ProtocolVersion;
use openmls_traits::types::Ciphersuite;

// TODO confirm ciphersuite choice
pub const CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519;

pub const MLS_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::Mls10;

pub const WELCOME_HPKE_LABEL: &str = "MLS_WELCOME";

pub const MAX_GROUP_SYNC_RETRIES: usize = 3;

pub const MAX_INTENT_PUBLISH_ATTEMPTS: usize = 3;

const NANOSECONDS_IN_HOUR: i64 = 3_600_000_000_000;

pub const UPDATE_INSTALLATIONS_INTERVAL_NS: i64 = NANOSECONDS_IN_HOUR / 2; // 30 min

pub const MAX_GROUP_SIZE: u8 = 250;

pub const MUTABLE_METADATA_EXTENSION_ID: u16 = 0xff00;
