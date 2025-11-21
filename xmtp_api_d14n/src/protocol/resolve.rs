//! Implementations for Dependency Resolution strategies [XIP](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#335-cross-originator-message-ordering)
//!
//! Possible Implementation of Dependency Resolution Strategies:
//! - keep retrying same query and error forever after and finish after some backoff
//! - query the originator that the mesasge is stored on.
//! - file misbehavior report if originator message came from is unresponsive
//! - dont resolve dependencies at all
//! - query random originators for the dependency
//! - round robin query for dependency
//! - etc.

mod network_backoff;
pub use network_backoff::*;
