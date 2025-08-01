use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::identity::api::v1::{
    FILE_DESCRIPTOR_SET, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
};
use xmtp_proto::xmtp::identity::associations::IdentityUpdate;

#[derive(Debug, Builder, Default)]
#[builder(build_fn(error = "BodyError"))]
pub struct PublishIdentityUpdate {
    #[builder(default)]
    pub identity_update: Option<IdentityUpdate>,
}

impl PublishIdentityUpdate {
    pub fn builder() -> PublishIdentityUpdateBuilder {
        Default::default()
    }
}

impl Endpoint for PublishIdentityUpdate {
    type Output = PublishIdentityUpdateResponse;
    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::Borrowed("/identity/v1/publish-identity-update")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<PublishIdentityUpdateRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(PublishIdentityUpdateRequest {
            identity_update: self.identity_update.clone(),
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        use xmtp_proto::xmtp::identity::api::v1::{
            FILE_DESCRIPTOR_SET, PublishIdentityUpdateRequest,
        };
        let _pnq = crate::path_and_query::<PublishIdentityUpdateRequest>(FILE_DESCRIPTOR_SET);
    }

    #[xmtp_common::test]
    async fn test_publish_identity_update() {
        use xmtp_common::time::now_ns;
        use xmtp_proto::xmtp::identity::associations::IdentityUpdate;

        let client = crate::TestClient::create_local();
        let client = client.build().await.unwrap();
        let endpoint = PublishIdentityUpdate::builder()
            .identity_update(Some(IdentityUpdate {
                actions: vec![],
                inbox_id: "".to_string(),
                client_timestamp_ns: now_ns() as u64,
            }))
            .build()
            .unwrap();

        let _: Result<PublishIdentityUpdateResponse, _> = endpoint.query(&client).await;
    }
}
