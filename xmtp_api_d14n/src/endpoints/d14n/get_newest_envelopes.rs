use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::xmtp::xmtpv4::message_api::{GetNewestEnvelopeRequest, GetNewestEnvelopeResponse};

/// Query a single thing
#[derive(Debug, Builder, Default, Clone)]
#[builder(build_fn(error = "BodyError"))]
pub struct GetNewestEnvelopes {
    #[builder(setter(each(name = "topic", into)))]
    topics: Vec<Vec<u8>>,
}

impl GetNewestEnvelopes {
    pub fn builder() -> GetNewestEnvelopesBuilder {
        Default::default()
    }
}

/// NOTE:insipx
/// Will get latest message for each topic
/// if there is no latest message, returns null in place of that message
/// ensure ordering is not affected by this null variable, or that extractors
/// do no unintentially skip nulls when they should preserve length.
impl Endpoint for GetNewestEnvelopes {
    type Output = GetNewestEnvelopeResponse;

    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::from("/mls/v2/get-newest-envelope")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<GetNewestEnvelopeRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        let query = GetNewestEnvelopeRequest {
            topics: self.topics.clone(),
        };
        Ok(query.encode_to_vec().into())
    }
}

#[cfg(test)]
mod test {
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        use xmtp_proto::xmtp::xmtpv4::message_api::GetNewestEnvelopeRequest;
        let pnq = xmtp_proto::path_and_query::<GetNewestEnvelopeRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn get_newest_envelopes() {
        use crate::d14n::GetNewestEnvelopes;

        let client = crate::TestGrpcClient::create_d14n();
        let client = client.build().await.unwrap();

        let mut endpoint = GetNewestEnvelopes::builder().topic(vec![]).build().unwrap();
        endpoint.query(&client).await.unwrap();
    }
}
