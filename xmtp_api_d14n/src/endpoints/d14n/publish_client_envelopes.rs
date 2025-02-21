use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::payer_api::FILE_DESCRIPTOR_SET;
use xmtp_proto::xmtp::xmtpv4::payer_api::PublishClientEnvelopesRequest;

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct PublishClientEnvelopes {
    #[builder(setter(into))]
    envelopes: Vec<ClientEnvelope>,
}

impl PublishClientEnvelopes {
    pub fn builder() -> PublishClientEnvelopesBuilder {
        Default::default()
    }
}

impl Endpoint for PublishClientEnvelopes {
    type Output = ();
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
