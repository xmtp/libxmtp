use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::mls_v1::QueryWelcomeMessagesResponse;
use xmtp_proto::xmtp::mls::api::v1::{PagingInfo, QueryWelcomeMessagesRequest};

#[derive(Debug, Builder, Default)]
#[builder(build_fn(error = "BodyError"))]
pub struct QueryWelcomeMessages {
    #[builder(setter(into))]
    installation_key: Vec<u8>,
    #[builder(setter(into), default)]
    paging_info: Option<PagingInfo>,
}

impl QueryWelcomeMessages {
    pub fn builder() -> QueryWelcomeMessagesBuilder {
        Default::default()
    }
}

impl Endpoint for QueryWelcomeMessages {
    type Output = QueryWelcomeMessagesResponse;
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<QueryWelcomeMessagesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(QueryWelcomeMessagesRequest {
            installation_key: self.installation_key.clone(),
            paging_info: self.paging_info,
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use crate::v3::QueryWelcomeMessages;
    use xmtp_proto::prelude::*;
    use xmtp_proto::xmtp::mls::api::v1::{
        QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse,
    };

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<QueryWelcomeMessagesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = QueryWelcomeMessages::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.mls.api.v1.MlsApi/QueryWelcomeMessages"
        );
    }

    #[xmtp_common::test]
    async fn test_query_welcome_messages() {
        let client = crate::TestClient::create_local();
        let client = client.build().await.unwrap();
        let mut endpoint = QueryWelcomeMessages::builder()
            .installation_key(vec![1, 2, 3])
            .build()
            .unwrap();

        let result: QueryWelcomeMessagesResponse = endpoint.query(&client).await.unwrap();
        assert_eq!(result.messages.len(), 0);
    }
}
