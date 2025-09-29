use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::xmtpv4::payer_api::{
    GetNodesRequest, GetNodesResponse,
};

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

    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::from("/mls/v2/payer/get-nodes")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<GetNodesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(GetNodesRequest {}
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::d14n::GetNodes;
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<GetNodesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_get_nodes() {
        let client = crate::TestClient::create_local_d14n();
        let client = client.build().await.unwrap();

        let endpoint = GetNodes::builder()
            .build()
            .unwrap();

        assert!(endpoint.query(&client).await.is_ok());
    }
}
