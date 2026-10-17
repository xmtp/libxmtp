use prost::{Message, bytes::Bytes};
use std::borrow::Cow;
use xmtp_proto::{
    api::{BodyError, Endpoint},
    backend_v1,
};

#[derive(Clone, Debug)]
pub struct Unregister(pub backend_v1::UnregisterRequest);

impl Endpoint for Unregister {
    type Output = backend_v1::UnregisterResponse;

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        "/xmtp.backend.v1.NotificationService/Unregister".into()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(self.0.encode_to_vec().into())
    }
}
