use prost::{Message, bytes::Bytes};
use std::borrow::Cow;
use xmtp_proto::{
    api::{BodyError, Endpoint},
    backend_v1,
};

#[derive(Clone, Debug)]
pub struct SubscribeStatic(pub backend_v1::SubscribeStaticRequest);

impl Endpoint for SubscribeStatic {
    type Output = backend_v1::SubscribeStaticResponse;
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        "/xmtp.backend.v1.SubscriptionService/SubscribeStatic".into()
    }
    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(self.0.encode_to_vec().into())
    }
}
