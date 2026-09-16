//! The unauthenticated configuration read (spec 006 §5, CFG-040).

use crate::{ApiClientWrapper, Result, dyn_err};
use xmtp_proto::{api_client::XmtpBackendClient, backend_v1 as wire};

impl<C: XmtpBackendClient> ApiClientWrapper<C> {
    /// Read what the deployment publishes about itself.
    ///
    /// Not retried here: the caller decides what a failure means. `build`
    /// fails with `ConfigurationUnavailable`; a refresh attempt logs and waits
    /// out its own schedule (CFG-046).
    #[xmtp_common::rpc_span]
    pub async fn get_configuration(&self) -> Result<wire::GetConfigurationResponse> {
        self.api_client
            .get_configuration(wire::GetConfigurationRequest {})
            .await
            .map_err(dyn_err)
    }

    /// The backend URL this client sends to, when the transport knows it.
    /// `None` skips the URL comparison of CFG-042 and CFG-055.
    pub fn backend_url(&self) -> Option<&str> {
        self.api_client.backend_url()
    }

    /// Whether a credential source was configured on the transport (CFG-062).
    pub fn has_credential_source(&self) -> bool {
        self.api_client.has_credential_source()
    }
}
