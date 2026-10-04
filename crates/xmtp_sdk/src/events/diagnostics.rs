use super::*;

#[xmtp_common::test(unwrap_try = true)]
fn event_attachment_diagnostics_hide_signed_urls() {
    let url = "https://example.invalid/file?token=event-attachment-access-token";
    let attachment = AttachmentRef {
        attachment_key: "attachment".into(),
        url: url.into(),
        content_digest: "digest".into(),
    };
    let failed = AttachmentFailed {
        attachment_key: attachment.attachment_key.clone(),
        url: attachment.url.clone(),
        content_digest: attachment.content_digest.clone(),
        cause: AttachmentFailureCause::LocalStorage,
    };
    let forms = [
        format!("{attachment:?}"),
        format!("{failed:?}"),
        format!(
            "{:?}",
            ClientEvent::AttachmentDeleted {
                attachment_deleted: attachment.clone()
            }
        ),
        format!(
            "{:?}",
            ClientEvent::AttachmentUploadFailed {
                attachment_upload_failed: failed.clone()
            }
        ),
        format!(
            "{:?}",
            vec![ClientEvent::AttachmentDownloadFailed {
                attachment_download_failed: failed.clone()
            }]
        ),
    ];
    for value in forms {
        assert!(
            !value.contains("event-attachment-access-token"),
            "attachment event diagnostic exposed signed URL: {value}"
        );
    }
    assert_eq!(attachment.url, url);
    assert_eq!(failed.url, url);
    assert_eq!(failed.cause, AttachmentFailureCause::LocalStorage);
}
