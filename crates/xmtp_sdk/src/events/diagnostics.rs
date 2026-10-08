use super::*;

#[xmtp_common::test(unwrap_try = true)]
fn event_attachment_diagnostics_hide_signed_urls() {
    let url = "https://example.invalid/file?token=event-attachment-access-token";
    let core_attachment = core::AttachmentRef {
        attachment_key: "attachment".into(),
        url: url.into(),
        content_digest: "digest".into(),
    };
    let core_failed = core::AttachmentFailed {
        attachment_key: core_attachment.attachment_key.clone(),
        url: core_attachment.url.clone(),
        content_digest: core_attachment.content_digest.clone(),
        cause: "local_storage".into(),
    };
    let core_ref_debug = format!("{core_attachment:?}");
    let core_failed_debug = format!("{core_failed:?}");
    let attachment: AttachmentRef = core_attachment.clone().into();
    let failed: AttachmentFailed = core_failed.clone().into();
    let forms = [
        core_ref_debug,
        core_failed_debug,
        format!("{attachment:?}"),
        format!("{failed:?}"),
        format!(
            "{:?}",
            core::ClientEvent::AttachmentDeleted(core_attachment.clone())
        ),
        format!(
            "{:?}",
            ClientEvent::AttachmentUploadFailed {
                attachment_upload_failed: failed.clone()
            }
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
    assert_eq!(failed.cause, "local_storage");
    assert_eq!(core_attachment.url, url);
    assert_eq!(core_failed.url, url);
    assert_eq!(core_failed.cause, "local_storage");
}
