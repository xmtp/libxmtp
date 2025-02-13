use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::FetchKeyPackagesRequest;
use xmtp_proto::xmtp::xmtpv4::message_api::FILE_DECRIPTOR_SET;
use xmtp_proto::xmtp::xmtpv4::message_api::{GetInboxIdsRequest, GetInboxIdsResponse};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct GetInboxIds {
    #[builder(setter(into))]
    envelopes: Vec<ClientEnvelope>,
}

impl GetInboxIds {
    pub fn builder() -> GetInboxIdsBuilder {
        Default::default()
    }
}

impl Endpoint for GetInboxIds {
    type Output = GetInboxIdsResponse;

    fn http_endpoint(&self) -> Cow<'static, str> {
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<PublishClientEnvelopesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(PublishClientEnvelopesRequest {
            envelopes: self.envelopes.clone(),
        }
        .encode_to_vec())
    }
}
