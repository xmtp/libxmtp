use crate::{Message, Timestamp, XmtpError};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use xmtp_db::group_message::{RecoveryPosition, RecoveryQueryArgs};

const PREFIX: &str = "mc1_";
const IDENTITY_BYTES: usize = 16;
const DEFAULT_PAGE_LIMIT: u32 = 50;
const ENCODED_CHUNK_BYTES: usize = 4096;

/// A sent-time boundary with an opaque database-local raw message key.
/// The boundary remains valid after deletion or publication of the row.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageRecoveryPosition {
    pub sent_at: Timestamp,
    pub message_cursor: String,
}

/// A chronological pending page and the raw prefix it consumed.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageRecoveryPage {
    /// Pending rows have no cursor until publication. An existing committed cursor is retained.
    pub messages: Vec<Message>,
    /// First consumed raw key, even when its message cannot be converted.
    pub first_position: Option<MessageRecoveryPosition>,
    /// Last consumed raw key. Use it for continuation in the same direction.
    pub last_position: Option<MessageRecoveryPosition>,
    /// An extra matching key exists. Its base body was not loaded or consumed.
    pub has_more: bool,
    /// Consumed raw rows minus converted messages.
    pub skipped_count: u32,
}

fn encode(position: RecoveryPosition) -> MessageRecoveryPosition {
    let mut bytes = position.database_id.to_vec();
    bytes.extend_from_slice(&position.message_id);
    MessageRecoveryPosition {
        sent_at: Timestamp(position.sent_at_ns),
        message_cursor: format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes)),
    }
}

fn parse(value: MessageRecoveryPosition) -> Result<RecoveryPosition, XmtpError> {
    if !value.message_cursor.starts_with(PREFIX) {
        return Err(super::cursor::invalid());
    }
    // Reuse the caller's buffer. Raw stored IDs have no fixed length limit.
    let mut bytes = value.message_cursor.into_bytes();
    let mut decoded = [0; ENCODED_CHUNK_BYTES / 4 * 3];
    let mut read = PREFIX.len();
    let mut written = 0;
    while read < bytes.len() {
        let end = (read + ENCODED_CHUNK_BYTES).min(bytes.len());
        // Full chunks end at a base64 quartet. The last chunk checks trailing bits.
        let count = URL_SAFE_NO_PAD
            .decode_slice(&bytes[read..end], &mut decoded)
            .map_err(|_| super::cursor::invalid())?;
        bytes[written..written + count].copy_from_slice(&decoded[..count]);
        written += count;
        read = end;
    }
    if written < IDENTITY_BYTES {
        return Err(super::cursor::invalid());
    }
    let database_id = bytes[..IDENTITY_BYTES]
        .try_into()
        .map_err(|_| super::cursor::invalid())?;
    bytes.copy_within(IDENTITY_BYTES..written, 0);
    bytes.truncate(written - IDENTITY_BYTES);
    Ok(RecoveryPosition {
        sent_at_ns: value.sent_at.0,
        database_id,
        message_id: bytes,
    })
}

pub(crate) fn recovery_query(
    options: Option<crate::ListMessagesOptions>,
    before: Option<MessageRecoveryPosition>,
    after: Option<MessageRecoveryPosition>,
) -> Result<RecoveryQueryArgs, XmtpError> {
    let mut options = options.unwrap_or_default();
    if options.limit == Some(0)
        || matches!(options.sort_by, Some(crate::MessageSortBy::InsertedAt))
        || matches!(
            options.delivery_status,
            Some(crate::DeliveryStatus::Published)
        )
    {
        return Err(XmtpError::invalid_argument(
            "Recovery pages require a positive limit, SentAt order and pending status.",
        ));
    }
    options.limit = Some(options.limit.unwrap_or(DEFAULT_PAGE_LIMIT));
    Ok(RecoveryQueryArgs {
        messages: options.try_into()?,
        before: before.map(parse).transpose()?,
        after: after.map(parse).transpose()?,
    })
}

pub(crate) fn lift_recovery_page(
    page: xmtp_mls::groups::message_list::EnrichedRecoveryPage,
    client_key: u64,
) -> MessageRecoveryPage {
    let consumed = page.messages.len();
    let messages = crate::conversation::lift_history_messages(page.messages, client_key);
    MessageRecoveryPage {
        skipped_count: (consumed - messages.len()) as u32,
        messages,
        first_position: page.first_position.map(encode),
        last_position: page.last_position.map(encode),
        has_more: page.has_more,
    }
}

#[cfg(test)]
mod tests;
