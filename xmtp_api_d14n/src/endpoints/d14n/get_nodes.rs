use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::xmtp::xmtpv4::payer_api::{GetNodesRequest, GetNodesResponse};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
pub struct GetNodes {}

impl GetNodes {
    pub fn builder() -> GetNodesBuilder {
        Default::default()
    }
}

impl Endpoint for GetNodes {
    type Output = GetNodesResponse;

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<GetNodesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(GetNodesRequest {}.encode_to_vec().into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::d14n::GetNodes;
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<GetNodesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_get_nodes() {
        let mut endpoint = GetNodes::builder().build().unwrap();
        let gateway_client = crate::TestGrpcClient::create_gateway();
        let client = gateway_client.build().await.unwrap();
        assert!(endpoint.query(&client).await.is_ok());
    }

    #[xmtp_common::test]
    async fn test_get_nodes_unimplemented() {
        // xmtpd doesn't implement the GetNodes endpoint
        let mut endpoint = GetNodes::builder().build().unwrap();
        let xmtpd_client = crate::TestGrpcClient::create_d14n();
        let client = xmtpd_client.build().await.unwrap();
        assert!(endpoint.query(&client).await.is_err());
    }
}
