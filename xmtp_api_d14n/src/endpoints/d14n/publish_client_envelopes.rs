use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
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
    use xmtp_proto::types::TopicKind;

    use super::*;
    use xmtp_api_grpc::{error::GrpcError, test::GatewayClient};
    use xmtp_common::rand_vec;
    use xmtp_proto::{
        api,
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
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = PublishClientEnvelopes::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.xmtpv4.payer_api.PayerApi/PublishClientEnvelopes"
        );
    }

    #[xmtp_common::test]
    async fn test_publish_client_envelopes() {
        use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;

        let client = GatewayClient::create();
        let client = client.build().unwrap();

        let aad = AuthenticatedData {
            target_topic: TopicKind::GroupMessagesV1.build(rand_vec::<16>()),
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

        let err = api::ignore(endpoint).query(&client).await.unwrap_err();
        // tracing::info!("{}", err);
        // the request will fail b/c we're using dummy data but
        // we just care if the endpoint is working
        match err {
            ApiClientError::<GrpcError>::ClientWithEndpoint {
                source: GrpcError::Status(ref s),
                ..
            } => {
                assert!(
                    s.message().contains("invalid payload")
                        || s.message().contains("invalid topic"),
                    "{}",
                    err
                );
            }
            _ => panic!("request failed"),
        }
    }
}
