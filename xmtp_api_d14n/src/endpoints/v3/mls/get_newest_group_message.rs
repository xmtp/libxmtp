use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::{GetNewestGroupMessageRequest, GetNewestGroupMessageResponse};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
pub struct GetNewestGroupMessage {
    #[builder(setter(into))]
    group_ids: Vec<Vec<u8>>,
    #[builder(default)]
    include_content: bool,
}

impl GetNewestGroupMessage {
    pub fn builder() -> GetNewestGroupMessageBuilder {
        Default::default()
    }
}

impl Endpoint for GetNewestGroupMessage {
    type Output = GetNewestGroupMessageResponse;

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<GetNewestGroupMessageRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(GetNewestGroupMessageRequest {
            group_ids: self.group_ids.clone(),
            include_content: self.include_content,
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use crate::v3::GetNewestGroupMessage;
    use xmtp_api_grpc::test::NodeGoClient;
    use xmtp_proto::prelude::*;
    use xmtp_proto::xmtp::mls::api::v1::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<GetNewestGroupMessageRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_get_newest_group_message() {
        let client = NodeGoClient::create();
        let client = client.build().unwrap();
        let mut endpoint = GetNewestGroupMessage::builder()
            .group_ids(vec![vec![1, 2, 3], vec![4, 5, 6]])
            .include_content(true)
            .build()
            .unwrap();

        let result = endpoint.query(&client).await.unwrap();
        assert_eq!(
            result.responses.len(),
            2,
            "Should return response for each group ID"
        );
    }

    #[xmtp_common::test]
    fn test_get_newest_group_message_builder() {
        // Test basic builder functionality
        let endpoint = GetNewestGroupMessage::builder()
            .group_ids(vec![vec![1, 2, 3]])
            .build()
            .unwrap();

        assert_eq!(endpoint.group_ids, vec![vec![1, 2, 3]]);
        assert!(!endpoint.include_content);
    }

    #[xmtp_common::test]
    fn test_get_newest_group_message_builder_with_content() {
        // Test builder with include_content set
        let endpoint = GetNewestGroupMessage::builder()
            .group_ids(vec![vec![7, 8, 9], vec![10, 11, 12]])
            .include_content(true)
            .build()
            .unwrap();

        assert_eq!(endpoint.group_ids, vec![vec![7, 8, 9], vec![10, 11, 12]]);
        assert!(endpoint.include_content);
    }

    #[xmtp_common::test]
    fn test_get_newest_group_message_endpoints() {
        let endpoint = GetNewestGroupMessage::builder()
            .group_ids(vec![vec![1]])
            .build()
            .unwrap();

        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.mls.api.v1.MlsApi/GetNewestGroupMessage"
        );
    }
}
