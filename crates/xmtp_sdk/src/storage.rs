use std::sync::Arc;

use crate::{ClientOptions, InboxID, XmtpError, client::CoreClient, conversation::on_sdk_worker};

#[derive(uniffi::Object)]
pub struct Storage {
    pub(crate) client: Arc<CoreClient>,
    pub(crate) options: ClientOptions,
    pub(crate) inbox_id: InboxID,
}

#[xmtp_macro::sdk_export]
impl Storage {
    pub async fn path(&self) -> Result<Option<String>, XmtpError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::client::native_storage_path(&self.options.storage, &self.inbox_id.0)
        }
        #[cfg(target_arch = "wasm32")]
        {
            crate::client::wasm_storage_path(&self.options.storage, &self.inbox_id.0)
        }
    }

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
            .path()
            .await?
            .ok_or_else(|| XmtpError::invalid("in-memory storage has no file"))?;
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            client.close().await.map_err(XmtpError::from_client)?;
            std::fs::remove_file(path).map_err(XmtpError::unknown)
        })
        .await
    }
}
