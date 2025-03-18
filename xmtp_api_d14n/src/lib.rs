mod endpoints;
pub use endpoints::*;

mod proto_cache;
pub(crate) use proto_cache::*;

pub mod compat;

#[cfg(any(test, feature = "test-utils"))]
pub use tests::*;
#[cfg(any(test, feature = "test-utils"))]
pub mod tests {
    #[cfg(test)]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
    // #[cfg(any(not(feature = "grpc-api"), not(feature = "http-api")))]
    // pub type TestClient = xmtp_proto::traits::mock::MockClient;
    #[cfg(not(any(feature = "http-api", target_arch = "wasm32")))]
    pub type TestClient = xmtp_api_grpc::grpc_client::GrpcClient;

    #[cfg(any(feature = "http-api", target_arch = "wasm32"))]
    pub type TestClient = xmtp_api_http::XmtpHttpApiClient;

    // Execute once before any tests are run
    #[cfg_attr(not(target_arch = "wasm32"), ctor::ctor)]
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(test)]
    fn _setup() {
        xmtp_common::logger();
    }
}
