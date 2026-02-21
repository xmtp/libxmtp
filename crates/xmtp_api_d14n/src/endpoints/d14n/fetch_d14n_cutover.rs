use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::xmtp::migration::api::v1::FetchD14nCutoverResponse;

#[derive(Debug, Default)]
pub struct FetchD14nCutover;

impl Endpoint for FetchD14nCutover {
    type Output = FetchD14nCutoverResponse;
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<FetchD14nCutoverResponse>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(pbjson_types::Empty::default().encode_to_vec().into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_api_grpc::test::NodeGoClient;
    use xmtp_proto::{api, prelude::*};

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = FetchD14nCutover;
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.migration.api.v1.D14nMigrationApi/FetchD14nCutover",
            "Expected correct grpc method path but got {}",
            endpoint.grpc_endpoint()
        );
    }

    // ignored until service implemented
    #[ignore]
    #[xmtp_common::test]
    async fn test_fetch_d14n_cutover() {
        let client = NodeGoClient::create();
        let client = client.build().unwrap();

        let endpoint = FetchD14nCutover;
        api::ignore(endpoint).query(&client).await.unwrap();
    }
}
