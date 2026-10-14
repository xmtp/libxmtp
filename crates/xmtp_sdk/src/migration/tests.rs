use super::*;
use prost::Message;
use xmtp_legacy_migration::{MigrationError, RecordError};

#[xmtp_common::test(unwrap_try = true)]
async fn record_failure_tells_the_caller_that_no_archive_was_published() {
    let decode_error =
        xmtp_proto::xmtp::message_contents::EncodedContent::decode(&b"\x0a\xff"[..]).unwrap_err();
    let private_detail = decode_error.to_string();
    for (cause, expected_reason) in [
        (
            RecordError::Invalid("invalid required deletion identity or Boolean"),
            Some("invalid required deletion identity or Boolean"),
        ),
        (RecordError::Content(decode_error), None),
    ] {
        let error = migration_error(MigrationError::RecordRead(cause));
        let display = error.to_string();
        let XmtpError::MigrationRecordRead(details) = error else {
            panic!("record failure changed category");
        };
        assert_eq!(details.code, "MigrationRecordRead");
        assert!(!details.retryable);
        assert!(details.message.contains("No new archive was published"));
        assert!(
            details
                .message
                .contains("Source data and any previous archive are unchanged")
        );
        assert!(display.contains("No new archive was published"));
        if let Some(reason) = expected_reason {
            assert!(details.message.contains(reason));
        }
        assert!(!display.contains(&private_detail));
    }
}
