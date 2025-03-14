mod endpoints;
pub use endpoints::*;

mod proto_cache;
pub(crate) use proto_cache::*;

// pub mod compat;

#[allow(unused)]
#[macro_use]
extern crate tracing;

#[cfg(any(test, feature = "test-utils"))]
pub use tests::*;
#[cfg(any(test, feature = "test-utils"))]
pub mod tests {
    // #[cfg(any(not(feature = "grpc-api"), not(feature = "http-api")))]
    // pub type TestClient = xmtp_proto::traits::mock::MockClient;

    #[cfg(all(feature = "grpc-api", not(feature = "http-api")))]
    pub type TestClient = xmtp_api_grpc::grpc_client::GrpcClient;

    #[cfg(all(feature = "http-api"))]
    pub type TestClient = xmtp_api_http::XmtpHttpApiClient;

    // Execute once before any tests are run
    #[cfg_attr(not(target_arch = "wasm32"), ctor::ctor)]
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(test)]
    fn _setup() {
        xmtp_common::logger();
    }
}
