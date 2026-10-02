use xmtp_mls::subscriptions::local_delivery::{DeliveryScope, LocalDelivery, LocalDeliveryFilter};

use crate::{Message, XmtpError, conversation::lift_history_messages};

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
    let snapshot = LocalDelivery::enriched_history_snapshot(context, scope, filter, limit)
        .map_err(super::delivery_error)?;
    let messages = lift_history_messages(snapshot.messages, client_key);
    Ok(MessageHistorySnapshot {
        messages,
        cursor: super::cursor::encode(snapshot.cursor),
    })
}
