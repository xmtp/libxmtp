use std::sync::Arc;
use tokio::sync::Mutex;
use xmtp_mls::subscriptions::{
    local_delivery::{DeliveryAcknowledgement, DeliveryScope, LocalDeliveryFilter},
    message_reader::{MessageReader as CoreMessageReader, MessageReaderControl},
};
use xmtp_proto::types::GroupId;

use crate::{Message, XmtpError};

#[derive(uniffi::Object)]
pub struct MessageReader {
    reader: Mutex<CoreMessageReader<xmtp_mls::MlsContext>>,
    control: MessageReaderControl,
    previous: Mutex<Option<DeliveryAcknowledgement<xmtp_mls::MlsContext>>>,
    client_key: u64,
}

impl MessageReader {
    pub(crate) fn open(
        context: xmtp_mls::MlsContext,
        group_id: GroupId,
        client_key: u64,
    ) -> Result<Arc<Self>, XmtpError> {
        let reader = CoreMessageReader::new(
            context,
            DeliveryScope::Groups(vec![group_id]),
            LocalDeliveryFilter::default(),
            None,
        )
        .map_err(XmtpError::unknown)?;
        let control = reader.control();
        Ok(Arc::new(Self {
            reader: Mutex::new(reader),
            control,
            previous: Mutex::new(None),
            client_key,
        }))
    }
}

impl Drop for MessageReader {
    fn drop(&mut self) {
        self.control.close();
    }
}

#[xmtp_macro::sdk_export]
impl MessageReader {
    /// Acknowledge the previous item when the next read starts.
    pub async fn next(&self) -> Result<Option<Message>, XmtpError> {
        let mut reader = self.reader.lock().await;
        if let Some(previous) = self.previous.lock().await.take() {
            previous.acknowledge().map_err(XmtpError::unknown)?;
        }
        let item = reader.next_delivery().await.map_err(XmtpError::unknown)?;
        match item {
            Some(item) => {
                let message = Message::from_stored(item.message, self.client_key)?;
                *self.previous.lock().await = Some(item.acknowledgement);
                Ok(Some(message))
            }
            None => Ok(None),
        }
    }

    pub async fn end(&self) -> Result<(), XmtpError> {
        self.control.close();
        if let Some(previous) = self.previous.lock().await.take() {
            previous.reject();
        }
        Ok(())
    }
}
