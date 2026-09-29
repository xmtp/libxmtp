//! The backend request that signs one attachment upload.

use crate::{ApiClientWrapper, Result, dyn_err};
use xmtp_proto::{
    api_client::XmtpBackendClient,
    backend_v1::{CreateUploadRequest, CreateUploadResponse},
};

impl<C: XmtpBackendClient> ApiClientWrapper<C> {
    /// Get the request to send to the configured storage target.
    #[xmtp_common::rpc_span]
    pub async fn create_upload(
        &self,
        request: CreateUploadRequest,
    ) -> Result<CreateUploadResponse> {
        self.retry_call(|| self.api_client.create_upload(request.clone()), false)
            .await
            .map_err(dyn_err)
    }
}
