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

pub mod api_client;
pub mod codec;
mod convert;
mod error;
mod impls;
mod proto_cache;
mod traits;
pub mod types;

pub use error::*;
pub use generated::*;
pub use proto_cache::*;

pub mod api {
    pub use super::traits::combinators::*;
    pub use super::traits::stream::*;
    pub use super::traits::*;
}

#[cfg(test)]
pub mod test {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    xmtp_common::if_native! {
        #[cfg(test)]
        #[ctor::ctor]
        fn _setup() {
            xmtp_common::logger()
        }
    }
}

pub mod prelude {
    pub use super::FILE_DESCRIPTOR_SET;
    xmtp_common::if_test! {
        pub use super::api_client::XmtpTestClient;
    }
    pub use super::api_client::{
        ApiBuilder, ArcedXmtpApi, BoxedXmtpApi, NetConnectConfig, XmtpIdentityClient,
        XmtpMlsClient, XmtpMlsStreams,
    };
    pub use super::traits::{ApiClientError, Client, Endpoint, Query, QueryStream};
}

pub mod identity_v1 {
    pub use super::xmtp::identity::api::v1::*;
}

pub mod mls_v1 {
    pub use super::xmtp::mls::api::v1::*;
}
