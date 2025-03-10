use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::identity::api::v1::{
    get_inbox_ids_request::Request, GetInboxIdsRequest, GetInboxIdsResponse, FILE_DESCRIPTOR_SET,
};
use xmtp_proto::xmtp::identity::associations::IdentifierKind;

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct GetInboxIds {
    #[builder(setter(into))]
    addresses: Vec<String>,
}

impl GetInboxIds {
    pub fn builder() -> GetInboxIdsBuilder {
        Default::default()
    }
}

impl Endpoint for GetInboxIds {
    type Output = GetInboxIdsResponse;
    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::from("/identity/v1/get-inbox-ids")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<GetInboxIdsRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(GetInboxIdsRequest {
            requests: self
                .addresses
                .iter()
                .cloned()
                .map(|i| Request {
                    identifier: i,
                    identifier_kind: IdentifierKind::Ethereum as i32,
                })
                .collect(),
        }
        .encode_to_vec())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_proto::prelude::*;

    #[test]
    fn test_file_descriptor() {
        use xmtp_proto::xmtp::identity::api::v1::{GetInboxIdsRequest, FILE_DESCRIPTOR_SET};

        let pnq = crate::path_and_query::<GetInboxIdsRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }

    #[tokio::test]
    async fn test_get_inbox_ids() {
        let client = crate::TestClient::create_local();
        let client = client.build().await.unwrap();
        let endpoint = GetInboxIds::builder()
            .addresses(vec![
                "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string()
            ])
            .build()
            .unwrap();

        let result = endpoint.query(&client).await.unwrap();
        assert_eq!(result.responses.len(), 1);
    }
}
