use prost::{Message, bytes::Bytes};
use std::borrow::Cow;
use xmtp_proto::{
    api::{BodyError, Endpoint},
    backend_v1,
};

/// Ask the backend for a signed request to store one ciphertext.
#[derive(Clone, Debug, Default)]
pub struct CreateUpload(pub backend_v1::CreateUploadRequest);

pub const CREATE_UPLOAD_PATH: &str = "/xmtp.backend.v1.AttachmentService/CreateUpload";

impl Endpoint for CreateUpload {
    type Output = backend_v1::CreateUploadResponse;

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        CREATE_UPLOAD_PATH.into()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(self.0.encode_to_vec().into())
    }
}
