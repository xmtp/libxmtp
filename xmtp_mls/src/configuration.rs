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

pub const DELIMITER: char = '\x01';

/// MLS Extension Types
///
/// Copied from draft-ietf-mls-protocol-16:
///
/// | Value            | Name                     | Message(s) | Recommended | Reference |
/// |:-----------------|:-------------------------|:-----------|:------------|:----------|
/// | 0x0000           | RESERVED                 | N/A        | N/A         | RFC XXXX  |
/// | 0x0001           | application_id           | LN         | Y           | RFC XXXX  |
/// | 0x0002           | ratchet_tree             | GI         | Y           | RFC XXXX  |
/// | 0x0003           | required_capabilities    | GC         | Y           | RFC XXXX  |
/// | 0x0004           | external_pub             | GI         | Y           | RFC XXXX  |
/// | 0x0005           | external_senders         | GC         | Y           | RFC XXXX  |
/// | 0xff00  - 0xffff | Reserved for Private Use | N/A        | N/A         | RFC XXXX  |
pub const MUTABLE_METADATA_EXTENSION_ID: u16 = 0xff00;
pub const GROUP_MEMBERSHIP_EXTENSION_ID: u16 = 0xff01;

pub const DEFAULT_GROUP_NAME: &str = "New Group";
pub const DEFAULT_GROUP_DESCRIPTION: &str = "New Group Description";
