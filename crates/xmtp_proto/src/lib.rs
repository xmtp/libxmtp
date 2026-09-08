#[allow(clippy::all)]
#[allow(warnings)]
mod generated {
    //! Module structure of Protos for XMTP

    include!(concat!(env!("OUT_DIR"), "/mod.rs"));
    pub const FILE_DESCRIPTOR_SET: &'static [u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/proto_descriptor.bin"));
}

pub mod api_client;
pub mod codec;
mod convert;
mod error;
mod impls;
mod traits;
pub mod types;

pub use error::*;
pub use generated::*;
pub use impls::update_dedupe::GroupUpdateDeduper;
pub use traits::short_hex::ShortHex;

pub mod api {
    pub use super::traits::combinators::*;
    pub use super::traits::stream::*;
    pub use super::traits::*;
}

#[cfg(test)]
pub mod test {
    xmtp_common::if_native! {
        #[cfg(test)]
        #[ctor::ctor(unsafe)]
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
        ApiBuilder, ArcedXmtpApi, BoxedXmtpApi, NetConnectConfig, XmtpBackendClient, XmtpMlsStreams,
    };
    pub use super::traits::{ApiClientError, BytesStream, Client, Endpoint, Query, QueryStream};
}

pub mod identity_v1 {
    pub use super::xmtp::identity::api::v1::*;
}

pub mod backend_v1 {
    pub use super::xmtp::backend::v1::*;
}

pub mod mls_v1 {
    pub use super::xmtp::mls::api::v1::*;
}
