use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::identity_v1::PublishIdentityUpdateResponse;
use xmtp_proto::xmtp::identity::api::v1::PublishIdentityUpdateRequest;
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
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<PublishIdentityUpdateRequest>()
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
    use xmtp_proto::{identity_v1::PublishIdentityUpdateResponse, prelude::*};

    #[xmtp_common::test]
    fn test_file_descriptor() {
        use xmtp_proto::xmtp::identity::api::v1::PublishIdentityUpdateRequest;
        let _pnq = xmtp_proto::path_and_query::<PublishIdentityUpdateRequest>();
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = PublishIdentityUpdate::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.identity.api.v1.IdentityApi/PublishIdentityUpdate"
        );
    }

    #[xmtp_common::test]
    async fn test_publish_identity_update() {
        use xmtp_common::time::now_ns;
        use xmtp_proto::xmtp::identity::associations::IdentityUpdate;

        let client = crate::TestGrpcClient::create_local();
        let client = client.build().unwrap();
        let mut endpoint = PublishIdentityUpdate::builder()
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
