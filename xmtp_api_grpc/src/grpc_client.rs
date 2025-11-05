mod client;
pub use client::{ClientBuilder, GrpcClient, GrpcStream};

#[cfg(any(test, feature = "test-utils"))]
mod test;

pub type GrpcClientBuilder = client::ClientBuilder;

xmtp_common::if_wasm! {
    mod wasm;
    pub use wasm::*;
    pub type GrpcService = wasm::GrpcWebService;
}

xmtp_common::if_native! {
    mod native;
    pub use native::*;
    pub type GrpcService = native::NativeGrpcService;
}
