use crate::{ApiClientWrapper, Result, dyn_err};
use xmtp_common::time::timeout;
use xmtp_configuration::NOTIFICATION_REQUEST_TIMEOUT;
use xmtp_proto::{
    api_client::XmtpBackendClient,
    backend_v1::{
        RecipientState, RegisterRequest, UnregisterRequest, UnregisterResponse,
        UpdateSubscriptionsRequest, register_request::Delivery,
    },
};
use zeroize::Zeroize;

/// Holds a request for the whole retry loop and wipes its secrets when the
/// call ends. Each attempt hands the transport a clone, which the transport
/// owns.
struct Wiped<T: WipeSecrets>(T);

impl<T: WipeSecrets> Drop for Wiped<T> {
    fn drop(&mut self) {
        self.0.wipe_secrets();
    }
}

trait WipeSecrets {
    fn wipe_secrets(&mut self);
}

impl WipeSecrets for RegisterRequest {
    fn wipe_secrets(&mut self) {
        self.recipient_secret.zeroize();
        if let Some(Delivery::Http(http)) = &mut self.delivery {
            http.signing_key.zeroize();
        }
    }
}

impl WipeSecrets for UnregisterRequest {
    fn wipe_secrets(&mut self) {
        self.recipient_secret.zeroize();
    }
}

impl WipeSecrets for UpdateSubscriptionsRequest {
    fn wipe_secrets(&mut self) {
        self.recipient_secret.zeroize();
        for subscription in &mut self.adds {
            subscription.hmac_keys.zeroize();
        }
    }
}

impl<C: XmtpBackendClient> ApiClientWrapper<C> {
    /// Create or renew a notification recipient.
    #[xmtp_common::rpc_span]
    pub async fn register(&self, request: RegisterRequest) -> Result<RecipientState> {
        let request = Wiped(request);
        timeout(
            NOTIFICATION_REQUEST_TIMEOUT,
            self.retry_call(|| self.api_client.register(request.0.clone()), false),
        )
        .await
        .map_err(xmtp_proto::api::ApiClientError::from)
        .map_err(dyn_err)?
        .map_err(dyn_err)
    }

    /// Delete a notification recipient and its subscriptions.
    #[xmtp_common::rpc_span]
    pub async fn unregister(&self, request: UnregisterRequest) -> Result<UnregisterResponse> {
        let request = Wiped(request);
        timeout(
            NOTIFICATION_REQUEST_TIMEOUT,
            self.retry_call(|| self.api_client.unregister(request.0.clone()), false),
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
        let request = Wiped(request);
        timeout(
            NOTIFICATION_REQUEST_TIMEOUT,
            self.retry_call(
                || self.api_client.update_subscriptions(request.0.clone()),
                false,
            ),
        )
        .await
        .map_err(xmtp_proto::api::ApiClientError::from)
        .map_err(dyn_err)?
        .map_err(dyn_err)
    }
}
