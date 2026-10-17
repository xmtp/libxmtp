#![warn(clippy::unwrap_used)]
mod endpoints;
pub use endpoints::*;
mod client;
pub use client::*;
pub mod envelope;
pub mod queries;
mod streams;
pub use queries::*;
pub mod middleware;
pub use middleware::*;
pub mod definitions;
pub use definitions::XmtpApiClient;
#[cfg(any(test, feature = "test-utils"))]
mod test;
xmtp_common::if_test! { pub use test::*; }
