use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::mls_v1::SubscribeWelcomeMessagesRequest;
use xmtp_proto::mls_v1::subscribe_welcome_messages_request::Filter as SubscribeFilter;

/// Query a single thing
#[derive(Debug, Builder, Default, Clone)]
#[builder(build_fn(error = "BodyError"))]
pub struct SubscribeWelcomeMessages {
    #[builder(setter(each(name = "filter", into)))]
    filters: Vec<SubscribeFilter>,
}

impl SubscribeWelcomeMessages {
    pub fn builder() -> SubscribeWelcomeMessagesBuilder {
        Default::default()
    }
}

impl Endpoint for SubscribeWelcomeMessages {
    type Output = crate::v3::types::V3ProtoWelcomeMessage;

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        Cow::Borrowed("/xmtp.mls.api.v1.MlsApi/SubscribeWelcomeMessages")
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        let query = SubscribeWelcomeMessagesRequest {
            filters: self.filters.clone(),
        };
        Ok(query.encode_to_vec().into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_api_grpc::test::NodeGoClient;
    use xmtp_proto::{api::QueryStreamExt, prelude::*};

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = SubscribeWelcomeMessages::default();
        assert!(!endpoint.grpc_endpoint().is_empty());
    }

    #[xmtp_common::test]
    async fn test_subscribe_envelopes() {
        let client = NodeGoClient::create();
        let client = client.build().unwrap();

        let mut endpoint = SubscribeWelcomeMessages::builder()
            .filter(SubscribeFilter {
                installation_key: vec![],
                id_cursor: 0,
            })
            .build()
            .unwrap();
        assert!(endpoint.subscribe(&client).await.is_ok());
    }
}
