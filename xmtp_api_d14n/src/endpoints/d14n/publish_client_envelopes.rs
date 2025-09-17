use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::payer_api::{
    PublishClientEnvelopesRequest, PublishClientEnvelopesResponse,
};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
pub struct PublishClientEnvelopes {
    #[builder(setter(each(name = "envelope", into)))]
    envelopes: Vec<ClientEnvelope>,
}

impl PublishClientEnvelopes {
    pub fn builder() -> PublishClientEnvelopesBuilder {
        Default::default()
    }
}

impl Endpoint for PublishClientEnvelopes {
    type Output = PublishClientEnvelopesResponse;
    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::from("/mls/v2/payer/publish-client-envelopes")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<PublishClientEnvelopesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(PublishClientEnvelopesRequest {
            envelopes: self.envelopes.clone(),
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use crate::protocol::TopicKind;

    use super::*;
    use xmtp_api_grpc::error::GrpcError;
    use xmtp_common::rand_vec;
    use xmtp_proto::{
        prelude::*,
        xmtp::xmtpv4::envelopes::{AuthenticatedData, client_envelope::Payload},
    };

    #[xmtp_common::test]
    fn test_file_descriptor() {
        use xmtp_proto::xmtp::xmtpv4::payer_api::PublishClientEnvelopesRequest;

        let pnq = xmtp_proto::path_and_query::<PublishClientEnvelopesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_publish_client_envelopes() {
        use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;

        let client = crate::TestClient::create_payer();
        let client = client.build().await.unwrap();

        let aad = AuthenticatedData {
            target_topic: TopicKind::GroupMessagesV1.build(&rand_vec::<16>()),
            depends_on: None,
        };
        let e = ClientEnvelope {
            aad: Some(aad),
            payload: Some(Payload::GroupMessage(Default::default())),
        };
        let endpoint = PublishClientEnvelopes::builder()
            .envelopes(vec![e])
            .build()
            .unwrap();

        let err = endpoint.query(&client).await.unwrap_err();
        tracing::info!("{}", err);
        // the request will fail b/c we're using dummy data but
        // we just care if the endpoint is working
        match err {
            ApiClientError::<GrpcError>::ClientWithEndpoint {
                source: GrpcError::Status(s),
                ..
            } => {
                assert!(s.message().contains("invalid payload"))
            }
            _ => panic!("request failed"),
        }
    }
}
