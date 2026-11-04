//! The MLS engine: groups with their intents, sync, commit log and welcomes,
//! and the stream receiver that feeds them (barrier, incoming, recovery and
//! connection state). Built on the core, identity, messages and server
//! configuration crates; names no facade or feature-crate type.
#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]

pub mod groups;
mod intents;
/// The stream receiver: one-shot barriers, incoming delivery, recovery and
/// connection state. The facade's `subscriptions` module re-exports these
/// modules at their old paths.
pub mod subscriptions {
    pub mod barrier;
    pub(crate) mod connection_state;
    pub mod incoming;
    pub(crate) mod recovery;
}

/// The context traits that the unit tests import.
#[cfg(test)]
mod prelude {
    pub use crate::groups::context::EngineFeatures;
    pub use xmtp_mls_core::context::CoreContext;
}
