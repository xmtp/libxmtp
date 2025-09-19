use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::mls_v1::FetchKeyPackagesResponse;
use xmtp_proto::xmtp::mls::api::v1::FetchKeyPackagesRequest;

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
pub struct FetchKeyPackages {
    #[builder(setter(into, each = "installation_key"))]
    installation_keys: Vec<Vec<u8>>,
}

impl FetchKeyPackages {
    pub fn builder() -> FetchKeyPackagesBuilder {
        Default::default()
    }
}

impl Endpoint for FetchKeyPackages {
    type Output = FetchKeyPackagesResponse;
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
    use xmtp_proto::{mls_v1::FetchKeyPackagesResponse, prelude::*};

    #[xmtp_common::test]
    fn test_file_descriptor() {
        use xmtp_proto::xmtp::mls::api::v1::FetchKeyPackagesRequest;
        let pnq = xmtp_proto::path_and_query::<FetchKeyPackagesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = FetchKeyPackages::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.mls.api.v1.MlsApi/FetchKeyPackages"
        );
    }

    #[xmtp_common::test]
    async fn test_fetch_key_packages() {
        let client = crate::TestGrpcClient::create_local();
        let client = client.build().unwrap();
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
