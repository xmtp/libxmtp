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
        Cow::Borrowed("/xmtp.migration.api.v1.D14nMigrationApi/FetchD14nCutover")
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(pbjson_types::Empty::default().encode_to_vec().into())
    }
}
