use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::{FetchKeyPackagesRequest, FetchKeyPackagesResponse};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
pub struct FetchKeyPackages {
    #[builder(setter(into))]
    installation_keys: Vec<Vec<u8>>,
}

impl FetchKeyPackages {
    pub fn builder() -> FetchKeyPackagesBuilder {
        Default::default()
    }
}

impl Endpoint for FetchKeyPackages {
    type Output = FetchKeyPackagesResponse;
    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::Borrowed("/mls/v1/fetch-key-packages")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<FetchKeyPackagesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(FetchKeyPackagesRequest {
            installation_keys: self.installation_keys.clone(),
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        use xmtp_proto::xmtp::mls::api::v1::FetchKeyPackagesRequest;
        let pnq = xmtp_proto::path_and_query::<FetchKeyPackagesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_fetch_key_packages() {
        let client = crate::TestClient::create_local();
        let client = client.build().await.unwrap();
        let mut endpoint = FetchKeyPackages::builder()
            .installation_keys(vec![vec![1, 2, 3]])
            .build()
            .unwrap();

        let result: FetchKeyPackagesResponse = endpoint.query(&client).await.unwrap();
        assert_eq!(
            result,
            FetchKeyPackagesResponse {
                key_packages: vec![Default::default()]
            }
        );
    }
}
