mod endpoints;
pub use endpoints::*;

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

    pub type TestClient = xmtp_api_grpc::GrpcClient;
    pub type ApiError = xmtp_api_grpc::error::GrpcError;

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

        fn create_local() -> Self::Builder {
            D14nClientBuilder::new(
                <C as XmtpTestClient>::create_d14n(),
                <Payer as XmtpTestClient>::create_payer(),
            )
        }
        fn create_dev() -> Self::Builder {
            // TODO: Staging
            panic!("no urls for d14n dev yet");
        }
        fn create_payer() -> Self::Builder {
            D14nClientBuilder::new(
                <C as XmtpTestClient>::create_payer(),
                <Payer as XmtpTestClient>::create_payer(),
            )
        }
        fn create_d14n() -> Self::Builder {
            D14nClientBuilder::new(
                <C as XmtpTestClient>::create_d14n(),
                <Payer as XmtpTestClient>::create_payer(),
            )
        }
    }

    impl<C> XmtpTestClient for V3Client<C>
    where
        C: XmtpTestClient + Client,
        <<C as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
    {
        type Builder = V3ClientBuilder<C::Builder>;
        fn create_local() -> Self::Builder {
            V3ClientBuilder::new(<C as XmtpTestClient>::create_local())
        }
        fn create_dev() -> Self::Builder {
            V3ClientBuilder::new(<C as XmtpTestClient>::create_dev())
        }
        fn create_payer() -> Self::Builder {
            V3ClientBuilder::new(<C as XmtpTestClient>::create_payer())
        }
        fn create_d14n() -> Self::Builder {
            V3ClientBuilder::new(<C as XmtpTestClient>::create_d14n())
        }
    }
}
