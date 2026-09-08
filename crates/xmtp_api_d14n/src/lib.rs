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
pub mod protocol;
pub use definitions::XmtpApiClient;
xmtp_common::if_test! { mod test; pub use test::*; }
