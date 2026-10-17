use prost::{Message, bytes::Bytes};
use std::borrow::Cow;
use xmtp_proto::{
    api::{BodyError, Endpoint},
    backend_v1,
};

#[derive(Clone, Debug)]
pub struct UpdateSubscriptions(pub backend_v1::UpdateSubscriptionsRequest);

impl Endpoint for UpdateSubscriptions {
    type Output = backend_v1::RecipientState;

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        "/xmtp.backend.v1.NotificationService/UpdateSubscriptions".into()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(self.0.encode_to_vec().into())
    }
}
