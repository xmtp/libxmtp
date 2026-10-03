use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use xmtp_db::delivery::DeliveryCursor;

use crate::{ErrorCategory, ErrorDetails, XmtpError};

const PREFIX: &str = "dc1_";
const CURSOR_BYTES: usize = 24;
const IDENTITY_BYTES: usize = 16;

pub(crate) fn encode(cursor: DeliveryCursor) -> String {
    let mut bytes = [0; CURSOR_BYTES];
    bytes[..IDENTITY_BYTES].copy_from_slice(&cursor.database_id);
    bytes[IDENTITY_BYTES..].copy_from_slice(&cursor.delivery_sequence.to_be_bytes());
    format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
}

pub(crate) fn invalid() -> XmtpError {
    XmtpError::InvalidCursor(ErrorDetails {
        code: "InvalidCursor".into(),
        category: ErrorCategory::Stream,
        retryable: false,
        message: "Use an unchanged cursor issued by this database.".into(),
        stream_failure: None,
    })
}

pub(crate) fn parse(value: &str) -> Result<DeliveryCursor, XmtpError> {
    let text = value.strip_prefix(PREFIX).ok_or_else(invalid)?;
    if text.len() != 32 {
        return Err(invalid());
    }
    let bytes = URL_SAFE_NO_PAD.decode(text).map_err(|_| invalid())?;
    let bytes: [u8; CURSOR_BYTES] = bytes.try_into().map_err(|_| invalid())?;
    let cursor = DeliveryCursor {
        database_id: bytes[..IDENTITY_BYTES].try_into().map_err(|_| invalid())?,
        delivery_sequence: u64::from_be_bytes(
            bytes[IDENTITY_BYTES..].try_into().map_err(|_| invalid())?,
        ),
    };
    if encode(cursor) != value {
        return Err(invalid());
    }
    Ok(cursor)
}
