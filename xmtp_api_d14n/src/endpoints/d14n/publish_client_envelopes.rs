use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::payer_api::FILE_DESCRIPTOR_SET;
use xmtp_proto::xmtp::xmtpv4::payer_api::{
    PublishClientEnvelopesRequest, PublishClientEnvelopesResponse,
};

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
    type Output = PublishClientEnvelopesResponse;
    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::from("/xmtp.xmtpv4.payer_api.PayerApi/PublishClientEnvelopes")
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

#[cfg(test)]
mod test {
    use crate::d14n::PublishClientEnvelopes;
    use xmtp_api_grpc::grpc_client::GrpcClient;
    use xmtp_api_grpc::LOCALHOST_ADDRESS;
    use xmtp_proto::api_client::ApiBuilder;
    use xmtp_proto::traits::Query;
    use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
    use xmtp_proto::xmtp::xmtpv4::payer_api::{PublishClientEnvelopesRequest, FILE_DESCRIPTOR_SET};

    #[test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<PublishClientEnvelopesRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }

    #[tokio::test]
    async fn test_get_inbox_ids() {
        let mut client = GrpcClient::builder();
        client.set_app_version("0.0.0".into()).unwrap();
        client.set_tls(false);
        client.set_host(LOCALHOST_ADDRESS.to_string());
        let client = client.build().await.unwrap();

        let endpoint = PublishClientEnvelopes::builder()
            .envelopes(vec![ClientEnvelope::default()])
            .build()
            .unwrap();

        // let result: PublishClientEnvelopesResponse = endpoint.query(&client).await.unwrap();
        // assert_eq!(result.originator_envelopes.len(), 0);
        //todo: fix later when it was implemented
        let result = endpoint.query(&client).await;
        assert!(result.is_err());
    }
}
