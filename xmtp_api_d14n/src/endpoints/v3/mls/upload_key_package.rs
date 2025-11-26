use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::{KeyPackageUpload, UploadKeyPackageRequest};

#[derive(Debug, Builder, Default)]
#[builder(build_fn(error = "BodyError"))]
pub struct UploadKeyPackage {
    key_package: Option<KeyPackageUpload>,
    #[builder(setter(into))]
    is_inbox_id_credential: bool,
}

impl UploadKeyPackage {
    pub fn builder() -> UploadKeyPackageBuilder {
        Default::default()
    }
}

impl Endpoint for UploadKeyPackage {
    type Output = ();
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<UploadKeyPackageRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(UploadKeyPackageRequest {
            key_package: self.key_package.clone(),
            is_inbox_id_credential: self.is_inbox_id_credential,
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use crate::v3::UploadKeyPackage;
    use xmtp_api_grpc::test::NodeGoClient;
    use xmtp_proto::xmtp::mls::api::v1::*;
    use xmtp_proto::{api, prelude::*};

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<UploadKeyPackageRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = UploadKeyPackage::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.mls.api.v1.MlsApi/UploadKeyPackage"
        );
    }

    #[xmtp_common::test]
    async fn test_upload_key_package() {
        let client = NodeGoClient::create();
        let client = client.build().unwrap();
        let endpoint = UploadKeyPackage::builder()
            .key_package(Some(KeyPackageUpload {
                key_package_tls_serialized: vec![1, 2, 3],
            }))
            .is_inbox_id_credential(false)
            .build()
            .unwrap();

        let result = api::ignore(endpoint).query(&client).await;
        assert!(result.is_err());
    }
}
