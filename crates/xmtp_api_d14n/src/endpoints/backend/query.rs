use prost::{Message, bytes::Bytes};
use std::borrow::Cow;
use xmtp_proto::{
    api::{BodyError, Endpoint},
    backend_v1,
};

#[derive(Clone, Debug)]
pub struct Query(pub backend_v1::QueryRequest);

impl Endpoint for Query {
    type Output = backend_v1::QueryResponse;
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        "/xmtp.backend.v1.QueryService/Query".into()
    }
    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(self.0.encode_to_vec().into())
    }
}
