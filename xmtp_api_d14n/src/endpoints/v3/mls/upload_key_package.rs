use std::borrow::Cow;
use derive_builder::Builder;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::{KeyPackageUpload, UploadKeyPackageRequest, FILE_DESCRIPTOR_SET};
use crate::QueryWelcomeMessagesBuilder;

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct UploadKeyPackage {
    #[builder(setter(strip_option))]
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
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<UploadKeyPackageRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        todo!()
    }
}
