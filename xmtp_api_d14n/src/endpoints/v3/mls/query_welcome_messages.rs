use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::FILE_DESCRIPTOR_SET;
use xmtp_proto::xmtp::mls::api::v1::{
    PagingInfo, QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse,
};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct QueryWelcomeMessages {
    #[builder(setter(into))]
    installation_key: Vec<u8>,
    #[builder(setter(into))]
    paging_info: Option<PagingInfo>,
}

impl QueryWelcomeMessages {
    pub fn builder() -> QueryWelcomeMessagesBuilder {
        Default::default()
    }
}

impl Endpoint for QueryWelcomeMessages {
    type Output = QueryWelcomeMessagesResponse;

    fn http_endpoint(&self) -> Cow<'static, str> {
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<QueryWelcomeMessagesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(QueryWelcomeMessagesRequest {
            installation_key: self.installation_key.clone(),
            paging_info: self.paging_info,
        }
        .encode_to_vec())
    }
}
