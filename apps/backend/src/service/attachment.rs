//! Issue one bounded upload request for an attachment ciphertext.

#[cfg(test)]
mod tests;

use crate::{Backend, api, error::Error};
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl api::attachment_service_server::AttachmentService for Backend {
    #[xmtp_common::rpc_span]
    // implements: ATCH-020, ATCH-021, ATCH-028
    async fn create_upload(
        &self,
        request: Request<api::CreateUploadRequest>,
    ) -> Result<Response<api::CreateUploadResponse>, Status> {
        let target = self
            .attachments
            .as_ref()
            .ok_or_else(|| Status::unimplemented("attachment storage is not configured"))?;
        let request = request.into_inner();
        let digest: [u8; 32] = request.content_digest.try_into().map_err(|_| {
            Status::invalid_argument("content_digest must contain exactly 32 bytes")
        })?;
        let max = self
            .config
            .attachments
            .as_ref()
            .expect("target has config")
            .upload_ceiling();
        if request.content_length == 0 || request.content_length > max {
            return Err(Status::invalid_argument(
                "content_length is outside the upload limit",
            ));
        }
        let signed = target
            .presign_put(&digest, request.content_length)
            .await
            .map_err(Error::from)?;
        Ok(Response::new(api::CreateUploadResponse {
            method: signed.method,
            url: signed.url,
            headers: signed
                .headers
                .into_iter()
                .map(|(name, value)| api::HttpHeader { name, value })
                .collect(),
            expires_in_seconds: signed.expires_in_seconds,
        }))
    }
}
