//! <https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#33-client-to-node-protocol>

pub mod traits;
pub use traits::*;

pub mod types;
pub use types::*;

pub mod envelopes;

pub mod extractors;
pub use extractors::*;

mod impls;

use openmls::prelude::ProtocolVersion;
pub const MLS_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::Mls10;
