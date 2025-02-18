use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::FILE_DESCRIPTOR_SET;
use xmtp_proto::xmtp::mls::api::v1::{FetchKeyPackagesRequest, FetchKeyPackagesResponse};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
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
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<FetchKeyPackagesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(FetchKeyPackagesRequest {
            installation_keys: self.installation_keys.clone(),
        }
        .encode_to_vec())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_api_grpc::{grpc_client::GrpcClient, LOCALHOST_ADDRESS};
    use xmtp_proto::api_client::ApiBuilder;
    use xmtp_proto::traits::Query;

    #[test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<FetchKeyPackagesRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }

    #[tokio::test]
    async fn test_fetch_key_packages() {
        let mut client = GrpcClient::builder();
        client.set_app_version("0.0.0".into()).unwrap();
        client.set_tls(false);
        client.set_host(LOCALHOST_ADDRESS.to_string());
        let client = client.build().await.unwrap();

        let endpoint = FetchKeyPackages::builder()
            .installation_keys(vec![vec![1, 2, 3]])
            .build()
            .unwrap();

        let result: FetchKeyPackagesResponse = endpoint.query(&client).await.unwrap();
        assert_eq!(result.key_packages, vec![]);
    }
}
