use crate::{ApiClientWrapper, Result, dyn_err};
use xmtp_common::time::timeout;
use xmtp_configuration::NOTIFICATION_REQUEST_TIMEOUT;
use xmtp_proto::{
    api_client::XmtpBackendClient,
    backend_v1::{
        RecipientState, RegisterRequest, UnregisterRequest, UnregisterResponse,
        UpdateSubscriptionsRequest,
    },
};

impl<C: XmtpBackendClient> ApiClientWrapper<C> {
    /// Create or renew a notification recipient.
    #[xmtp_common::rpc_span]
    pub async fn register(&self, request: RegisterRequest) -> Result<RecipientState> {
        timeout(
            NOTIFICATION_REQUEST_TIMEOUT,
            self.retry_call(|| self.api_client.register(request.clone()), false),
        )
        .await
        .map_err(xmtp_proto::api::ApiClientError::from)
        .map_err(dyn_err)?
        .map_err(dyn_err)
    }

    /// Delete a notification recipient and its subscriptions.
    #[xmtp_common::rpc_span]
    pub async fn unregister(&self, request: UnregisterRequest) -> Result<UnregisterResponse> {
        timeout(
            NOTIFICATION_REQUEST_TIMEOUT,
            self.retry_call(|| self.api_client.unregister(request.clone()), false),
        )
        .await
        .map_err(xmtp_proto::api::ApiClientError::from)
        .map_err(dyn_err)?
        .map_err(dyn_err)
    }

    /// Atomically update notification subscriptions and renew the recipient.
    #[xmtp_common::rpc_span]
    pub async fn update_subscriptions(
        &self,
        request: UpdateSubscriptionsRequest,
    ) -> Result<RecipientState> {
        timeout(
            NOTIFICATION_REQUEST_TIMEOUT,
            self.retry_call(
                || self.api_client.update_subscriptions(request.clone()),
                false,
            ),
        )
        .await
        .map_err(xmtp_proto::api::ApiClientError::from)
        .map_err(dyn_err)?
        .map_err(dyn_err)
    }
}
