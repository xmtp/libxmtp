use std::sync::Arc;
use xmtp_mls::context::XmtpSharedContext;

#[cfg(not(target_arch = "wasm32"))]
use crate::conversation::on_sdk_worker;
use crate::{
    XmtpError,
    client::{CoreClient, EventReaderRegistry, end_client},
};

#[derive(uniffi::Object)]
pub struct Storage {
    pub(crate) client: Arc<CoreClient>,
    pub(crate) path: Option<String>,
    pub(crate) listeners: Arc<crate::events::dispatch::ListenerRegistry>,
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

    pub async fn delete(&self) -> Result<(), XmtpError> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| XmtpError::invalid("in-memory storage has no file"))?;
        if !self.client.context.shutdown_complete() {
            end_client(&self.client, &self.listeners, &self.event_readers).await?;
        }
        std::fs::remove_file(path).map_err(XmtpError::unknown)
    }
}
