#[allow(clippy::all)]
#[allow(warnings)]
mod generated {
    //! Module structure of Protos for XMTP
    //!
    //! Edit the 'build.rs' file and uncomment '.include_file' to generate this file
    //! from the beginning. Generating this file anew will remove all ".serde.rs" includes,
    //! since pbjson does not integrate with prost/tonic build
    include!("gen/mod.rs");
    pub const FILE_DESCRIPTOR_SET: &'static [u8] = include_bytes!("gen/proto_descriptor.bin");
}

pub use generated::*;

mod error;
mod impls;

pub use error::*;

pub mod api_client;
#[cfg(any(test, feature = "test-utils"))]
pub use api_client::tests::*;

pub mod codec;
pub mod traits;

#[cfg(feature = "convert")]
pub mod convert;
#[cfg(feature = "convert")]
pub mod types;
#[cfg(feature = "convert")]
pub mod v4_utils;

#[cfg(test)]
pub mod test {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
}

pub mod prelude {
    #[cfg(any(test, feature = "test-utils"))]
    pub use super::api_client::XmtpTestClient;
    pub use super::api_client::{
        ApiBuilder, ArcedXmtpApi, BoxedXmtpApi, XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams,
    };
    pub use super::traits::{ApiClientError, Client, Endpoint, Query};
    pub use super::FILE_DESCRIPTOR_SET;
}

pub mod identity_v1 {
    pub use super::xmtp::identity::api::v1::*;
}

pub mod mls_v1 {
    pub use super::xmtp::mls::api::v1::*;
}
