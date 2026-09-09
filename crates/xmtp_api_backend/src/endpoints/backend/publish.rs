use prost::{Message, bytes::Bytes};
use std::borrow::Cow;
use xmtp_proto::{
    api::{BodyError, Endpoint},
    backend_v1,
};

#[derive(Clone, Debug)]
pub struct Publish(pub backend_v1::PublishRequest);

impl Endpoint for Publish {
    type Output = backend_v1::PublishResponse;
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        "/xmtp.backend.v1.PublishService/Publish".into()
    }
    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(self.0.encode_to_vec().into())
    }
}
