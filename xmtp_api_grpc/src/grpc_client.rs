mod client;
pub use client::{ClientBuilder, GrpcClient, GrpcStream};

xmtp_common::if_test! {
    pub mod test;
}

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
