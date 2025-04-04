use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::{
    FILE_DESCRIPTOR_SET, KeyPackageUpload, UploadKeyPackageRequest,
};

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
    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::Borrowed("/mls/v1/upload-key-package")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<UploadKeyPackageRequest>(FILE_DESCRIPTOR_SET)
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
    use xmtp_proto::prelude::*;
    use xmtp_proto::xmtp::mls::api::v1::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<UploadKeyPackageRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_get_identity_updates_v2() {
        let client = crate::TestClient::create_local();
        let client = client.build().await.unwrap();
        let endpoint = UploadKeyPackage::builder()
            .key_package(Some(KeyPackageUpload {
                key_package_tls_serialized: vec![1, 2, 3],
            }))
            .is_inbox_id_credential(false)
            .build()
            .unwrap();

        //todo: fix later when it was implemented
        let result = endpoint.query(&client).await;
        assert!(result.is_err());
    }
}
