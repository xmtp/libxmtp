use std::collections::HashMap;

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
    // Use the same relation and deletion projection as ordinary message reads.
    // Keep the selected rows, their order, and the atomic replay boundary.
    let mut groups = HashMap::new();
    let mut projected = vec![None; snapshot.messages.len()];
    for (index, item) in snapshot.messages.into_iter().enumerate() {
        groups
            .entry(item.message.group_id.clone())
            .or_insert_with(Vec::new)
            .push(((index, item.cursor), item.message));
    }
    for (group_id, items) in groups {
        let (positions, rows): (Vec<_>, Vec<_>) = items.into_iter().unzip();
        let enriched = xmtp_mls::messages::enrichment::enrich_messages_with_stored(
            context.db(),
            &group_id,
            rows,
        )
        .map_err(XmtpError::from_core)?;
        if enriched.len() != positions.len() {
            return Err(XmtpError::unknown(
                "message enrichment changed the selected row count",
            ));
        }
        for ((index, cursor), row) in positions.into_iter().zip(enriched) {
            projected[index] = Some(
                Message::from_enriched(row.stored, row.decoded, row.parent_stored, client_key)?
                    .with_delivery_cursor(Some(cursor)),
            );
        }
    }
    let messages = projected
        .into_iter()
        .map(|message| {
            message.ok_or_else(|| XmtpError::unknown("message enrichment omitted a selected row"))
        })
        .collect::<Result<_, _>>()?;
    Ok(MessageHistorySnapshot {
        messages,
        cursor: super::cursor::encode(snapshot.cursor),
    })
}
