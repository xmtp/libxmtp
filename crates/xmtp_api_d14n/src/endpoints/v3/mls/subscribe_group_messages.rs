use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::mls_v1::SubscribeGroupMessagesRequest;
use xmtp_proto::mls_v1::subscribe_group_messages_request::Filter as SubscribeFilter;

/// Query a single thing
#[derive(Debug, Builder, Default, Clone)]
#[builder(build_fn(error = "BodyError"))]
pub struct SubscribeGroupMessages {
    #[builder(setter(each(name = "filter", into)))]
    filters: Vec<SubscribeFilter>,
}

impl SubscribeGroupMessages {
    pub fn builder() -> SubscribeGroupMessagesBuilder {
        Default::default()
    }
}

impl Endpoint for SubscribeGroupMessages {
    type Output = crate::v3::types::V3ProtoGroupMessage;

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<SubscribeGroupMessagesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        let query = SubscribeGroupMessagesRequest {
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
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<SubscribeGroupMessagesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_subscribe_envelopes() {
        let client = NodeGoClient::create();
        let client = client.build().unwrap();

        let mut endpoint = SubscribeGroupMessages::builder()
            .filter(SubscribeFilter {
                group_id: vec![],
                id_cursor: 0,
            })
            .build()
            .unwrap();
        assert!(endpoint.subscribe(&client).await.is_ok());
    }
}
