use super::ClientEvent;
use xmtp_common::{MaybeSend, MaybeSync};

#[xmtp_macro::callback_error]
#[derive(Clone, Debug, thiserror::Error, uniffi::Error)]
pub enum ListenerError {
    #[error("event listener failed")]
    Failed,
}

impl From<uniffi::UnexpectedUniFFICallbackError> for ListenerError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed
    }
}

#[uniffi::export(with_foreign)]
#[xmtp_common::async_trait]
pub trait EventListener: MaybeSend + MaybeSync + 'static {
    async fn on_event(&self, event: ClientEvent) -> Result<(), ListenerError>;
}
