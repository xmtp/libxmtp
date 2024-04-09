#[allow(clippy::all)]
mod generated {
    include!("gen/mod.rs");
}

pub use generated::*;

// pub use gen::*;
#[cfg(feature = "xmtp-message_api-v1")]
pub mod api_client;
