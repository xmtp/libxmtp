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
        Cow::Borrowed("/xmtp.xmtpv4.payer_api.PayerApi/GetNodes")
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(GetNodesRequest {}.encode_to_vec().into())
    }
}
