use xmtp_mls::subscriptions::local_delivery::{DeliveryScope, LocalDelivery, LocalDeliveryFilter};

use crate::{Message, XmtpError};

/// Retained messages and their replay boundary from one database snapshot.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageHistorySnapshot {
    pub messages: Vec<Message>,
    pub cursor: String,
}

pub(crate) fn history_snapshot(
    context: &xmtp_mls::MlsContext,
    scope: &DeliveryScope,
    filter: &LocalDeliveryFilter,
    limit: u32,
    client_key: u64,
) -> Result<MessageHistorySnapshot, XmtpError> {
    // implements: DMS-016
    let snapshot = LocalDelivery::history_snapshot(context, scope, filter, limit)
        .map_err(super::delivery_error)?;
    let messages = snapshot
        .messages
        .into_iter()
        .map(|item| {
            Message::from_stored(item.message, client_key)
                .map(|message| message.with_delivery_cursor(Some(item.cursor)))
        })
        .collect::<Result<_, _>>()?;
    Ok(MessageHistorySnapshot {
        messages,
        cursor: super::cursor::encode(snapshot.cursor),
    })
}
