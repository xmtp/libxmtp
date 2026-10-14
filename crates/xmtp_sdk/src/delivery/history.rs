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

/// A sent-time boundary with a committed database-local delivery position.
/// Keep the cursor opaque. A retained boundary remains valid after deletion.
/// A whole-database restore or another database rejects the cursor.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageHistoryPosition {
    /// Exact signed nanoseconds from the boundary message or consumed raw key.
    pub sent_at: crate::Timestamp,
    /// The existing opaque delivery cursor from the same message or raw key.
    pub delivery_cursor: String,
}

/// A chronological Published page and the raw prefix it consumed.
/// First and last positions follow the requested output direction.
/// Empty messages can still have consumed positions and more history.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageHistoryPage {
    /// Readable messages in sent-time and local-delivery order.
    pub messages: Vec<Message>,
    /// First consumed raw key, including when its message cannot be converted.
    pub first_position: Option<MessageHistoryPosition>,
    /// Last consumed raw key. Use it for the next page in the same direction.
    pub last_position: Option<MessageHistoryPosition>,
    /// One more eligible key exists. Its base body is not loaded or consumed.
    pub has_more: bool,
    /// Consumed raw rows minus converted messages.
    pub skipped_count: u32,
}

const DEFAULT_PAGE_LIMIT: u32 = 50;

pub(crate) fn page_query(
    options: Option<crate::ListMessagesOptions>,
    before: Option<MessageHistoryPosition>,
    after: Option<MessageHistoryPosition>,
) -> Result<xmtp_db::group_message::MsgQueryArgs, XmtpError> {
    let mut options = options.unwrap_or_default();
    if options.limit == Some(0)
        || matches!(options.sort_by, Some(crate::MessageSortBy::InsertedAt))
        || matches!(
            options.delivery_status,
            Some(crate::DeliveryStatus::Failed | crate::DeliveryStatus::Unpublished)
        )
    {
        return Err(XmtpError::invalid_argument(
            "History pages require a positive limit, SentAt order and Published status.",
        ));
    }
    options.limit = Some(options.limit.unwrap_or(DEFAULT_PAGE_LIMIT));
    options.delivery_status = Some(crate::DeliveryStatus::Published);
    let mut query: xmtp_db::group_message::MsgQueryArgs = options.try_into()?;
    query.sort_by = Some(xmtp_db::group_message::SortBy::SentAtDelivery);
    fn position(
        value: MessageHistoryPosition,
    ) -> Result<xmtp_db::delivery::HistoryPosition, XmtpError> {
        Ok(xmtp_db::delivery::HistoryPosition {
            sent_at_ns: value.sent_at.0,
            cursor: super::cursor::parse(&value.delivery_cursor)?,
        })
    }
    query.history_before = before.map(position).transpose()?;
    query.history_after = after.map(position).transpose()?;
    Ok(query)
}

pub(crate) fn lift_page(
    page: xmtp_mls::groups::message_list::EnrichedHistoryPage,
    client_key: u64,
) -> MessageHistoryPage {
    let consumed = page.messages.len();
    let messages = crate::conversation::lift_history_messages(page.messages, client_key);
    let position = |value: xmtp_db::delivery::HistoryPosition| MessageHistoryPosition {
        sent_at: crate::Timestamp(value.sent_at_ns),
        delivery_cursor: super::cursor::encode(value.cursor),
    };
    MessageHistoryPage {
        skipped_count: (consumed - messages.len()) as u32,
        messages,
        first_position: page.first_position.map(&position),
        last_position: page.last_position.map(&position),
        has_more: page.has_more,
    }
}

#[cfg(test)]
pub(super) mod tests;

pub(crate) fn history_error(
    error: xmtp_mls::messages::enrichment::EnrichMessageError,
) -> XmtpError {
    use xmtp_db::{StorageError, stream_storage::StreamStorageError};
    use xmtp_mls::messages::enrichment::EnrichMessageError;
    match error {
        EnrichMessageError::Storage(StorageError::Stream(
            StreamStorageError::InvalidDeliveryPosition,
        )) => super::cursor::invalid(),
        EnrichMessageError::Storage(storage) => super::delivery_error(
            xmtp_mls::subscriptions::local_delivery::LocalDeliveryError::Storage(storage),
        ),
        other => XmtpError::from_core(other),
    }
}
