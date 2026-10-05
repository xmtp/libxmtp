#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use xmtp_mls::context::XmtpSharedContext;

use crate::XmtpError;
#[cfg(not(target_arch = "wasm32"))]
use crate::{
    client::{CoreClient, EventReaderRegistry, end_client},
    conversation::on_sdk_worker,
};

#[derive(uniffi::Object)]
pub struct Storage {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) client: Arc<CoreClient>,
    pub(crate) path: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) listeners: Arc<crate::events::dispatch::ListenerRegistry>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) event_readers: Arc<parking_lot::Mutex<EventReaderRegistry>>,
}

#[xmtp_macro::sdk_export]
impl Storage {
    pub async fn path(&self) -> Result<Option<String>, XmtpError> {
        Ok(self.path.clone())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_macro::sdk_export]
impl Storage {
    pub async fn reconnect(&self) -> Result<(), XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            client.reconnect_db().map_err(XmtpError::from_client)
        })
        .await
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_macro::sdk_export]
impl Storage {
    pub async fn delete(&self) -> Result<(), XmtpError> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| XmtpError::invalid("in-memory storage has no file"))?;
        if !self.client.context.shutdown_complete() {
            end_client(&self.client, &self.listeners, &self.event_readers).await?;
        }
        std::fs::remove_file(path).map_err(XmtpError::storage)
    }
}
