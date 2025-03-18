use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::identity::associations::IdentifierKind;
use xmtp_proto::xmtp::xmtpv4::message_api::FILE_DESCRIPTOR_SET;
use xmtp_proto::xmtp::xmtpv4::message_api::{
    get_inbox_ids_request, GetInboxIdsRequest, GetInboxIdsResponse,
};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct GetInboxIds {
    #[builder(setter(into))]
    addresses: Vec<String>,
    #[builder(setter(into), default)]
    passkeys: Vec<String>,
}

impl GetInboxIds {
    pub fn builder() -> GetInboxIdsBuilder {
        Default::default()
    }
}

impl Endpoint for GetInboxIds {
    type Output = GetInboxIdsResponse;

    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::from("/mls/v2/get-inbox-ids")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<GetInboxIdsRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        let addresses = self
            .addresses
            .iter()
            .cloned()
            .map(|a| (a, IdentifierKind::Ethereum));
        let passkeys = self
            .passkeys
            .iter()
            .cloned()
            .map(|p| (p, IdentifierKind::Passkey));

        Ok(GetInboxIdsRequest {
            requests: addresses
                .chain(passkeys)
                .map(|(i, kind)| get_inbox_ids_request::Request {
                    identifier: i,
                    identifier_kind: kind as i32,
                })
                .collect(),
        }
        .encode_to_vec())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::d14n::GetInboxIds;
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<GetInboxIdsRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_get_inbox_ids() {
        let client = crate::TestClient::create_local_d14n();
        let client = client.build().await.unwrap();

        let endpoint = GetInboxIds::builder()
            .addresses(vec![
                "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string()
            ])
            .build()
            .unwrap();

        assert!(endpoint.query(&client).await.is_ok());
    }
}
