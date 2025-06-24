mod endpoints;
pub use endpoints::*;

mod proto_cache;
pub(crate) use proto_cache::*;

pub mod queries;
pub use queries::*;

pub mod protocol;

#[cfg(any(test, feature = "test-utils"))]
pub use tests::*;
#[cfg(any(test, feature = "test-utils"))]
pub mod tests {

    use xmtp_proto::{
        prelude::{ApiBuilder, XmtpTestClient},
        traits::Client,
    };

    use crate::{D14nClient, D14nClientBuilder, V3Client, V3ClientBuilder};

    #[cfg(test)]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
    // #[cfg(any(not(feature = "grpc-api"), not(feature = "http-api")))]
    // pub type TestClient = xmtp_proto::traits::mock::MockClient;
    #[cfg(not(any(feature = "http-api", target_arch = "wasm32")))]
    pub type TestClient = xmtp_api_grpc::grpc_client::GrpcClient;

    #[cfg(any(feature = "http-api", target_arch = "wasm32"))]
    pub type TestClient = xmtp_api_http::XmtpHttpApiClient;

    #[cfg(not(any(feature = "http-api", target_arch = "wasm32")))]
    pub type ApiError = xmtp_api_grpc::GrpcError;

    #[cfg(any(feature = "http-api", target_arch = "wasm32"))]
    pub type ApiError = xmtp_api_http::HttpClientError;

    pub type TestV3Client = V3Client<TestClient>;
    pub type TestD14nClient = D14nClient<TestClient, TestClient>;

    impl<C, Payer> XmtpTestClient for D14nClient<C, Payer>
    where
        C: XmtpTestClient + Client,
        Payer: XmtpTestClient + Client,
        <<C as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
        <<Payer as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
        <C as XmtpTestClient>::Builder:
            ApiBuilder<Error = <<Payer as XmtpTestClient>::Builder as ApiBuilder>::Error>,
    {
        type Builder = D14nClientBuilder<C::Builder, Payer::Builder>;

        fn local_port() -> &'static str {
            "5055"
        }

        fn create_custom(_addr: &str) -> Self::Builder {
            D14nClientBuilder::new(
                <C as XmtpTestClient>::create_local_d14n(),
                <Payer as XmtpTestClient>::create_local_d14n(),
            )
        }

        fn create_local() -> Self::Builder {
            D14nClientBuilder::new(
                <C as XmtpTestClient>::create_local_d14n(),
                <Payer as XmtpTestClient>::create_local_d14n(),
            )
        }
        fn create_dev() -> Self::Builder {
            // TODO: Staging
            D14nClientBuilder::new(
                <C as XmtpTestClient>::create_dev(),
                <Payer as XmtpTestClient>::create_dev(),
            )
        }
        fn create_local_payer() -> Self::Builder {
            D14nClientBuilder::new(
                <C as XmtpTestClient>::create_local_payer(),
                <Payer as XmtpTestClient>::create_local_payer(),
            )
        }
        fn create_local_d14n() -> Self::Builder {
            D14nClientBuilder::new(
                <C as XmtpTestClient>::create_local_d14n(),
                <Payer as XmtpTestClient>::create_local_d14n(),
            )
        }
    }

    impl<C> XmtpTestClient for V3Client<C>
    where
        C: XmtpTestClient + Client,
        <<C as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
    {
        type Builder = V3ClientBuilder<C::Builder>;

        fn local_port() -> &'static str {
            "5055"
        }

        fn create_custom(addr: &str) -> Self::Builder {
            V3ClientBuilder::new(<C as XmtpTestClient>::create_custom(addr))
        }

        fn create_local() -> Self::Builder {
            V3ClientBuilder::new(<C as XmtpTestClient>::create_local())
        }
        fn create_dev() -> Self::Builder {
            V3ClientBuilder::new(<C as XmtpTestClient>::create_dev())
        }
        fn create_local_payer() -> Self::Builder {
            V3ClientBuilder::new(<C as XmtpTestClient>::create_local_payer())
        }
        fn create_local_d14n() -> Self::Builder {
            V3ClientBuilder::new(<C as XmtpTestClient>::create_local_d14n())
        }
    }
}
