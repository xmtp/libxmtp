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
    let snapshot = LocalDelivery::enriched_history_snapshot(context, scope, filter, limit)
        .map_err(super::delivery_error)?;
    // A snapshot cursor covers every selected row. If one row cannot be
    // converted, return no cursor so the caller cannot skip that row.
    let messages = snapshot
        .messages
        .into_iter()
        .map(|enriched| {
            Message::from_enriched(
                enriched.stored,
                enriched.decoded,
                enriched.parent_stored,
                client_key,
            )
            .map(|message| message.with_delivery_cursor(enriched.delivery_cursor))
        })
        .collect::<Result<Vec<_>, XmtpError>>()?;
    Ok(MessageHistorySnapshot {
        messages,
        cursor: super::cursor::encode(snapshot.cursor),
    })
}
