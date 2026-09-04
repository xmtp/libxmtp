use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::xmtp::identity::associations::IdentifierKind;
use xmtp_proto::xmtp::xmtpv4::message_api::{
    GetInboxIdsRequest, GetInboxIdsResponse, get_inbox_ids_request,
};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
pub struct GetInboxIds {
    #[builder(setter(into), default)]
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
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        Cow::Borrowed("/xmtp.xmtpv4.message_api.ReplicationApi/GetInboxIds")
    }

    fn body(&self) -> Result<Bytes, BodyError> {
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
        .encode_to_vec()
        .into())
    }
}
