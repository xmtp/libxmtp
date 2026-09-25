use super::*;

// verifies: ATCH-002, ATCH-008
#[xmtp_common::test(unwrap_try = true)]
fn base_url_accepts_raw_rfc_forms() {
    for base_url in [
        "https://example.com/attachments",
        "https://example.com",
        "https://CDN.example.com/att",
        "https://example.com:443/att",
        "https://example.com/att%20file",
        "http://LOCALHOST/files",
        "http://127.0.0.1:9000",
        "http://127.0.0.1/attachments",
        "http://[::1]/attachments",
        "http://[0:0:0:0:0:0:0:1]/att",
    ] {
        assert_eq!(check_base_url(base_url), Ok(()), "{base_url}");
    }
}

// verifies: ATCH-002, ATCH-008
#[xmtp_common::test(unwrap_try = true)]
fn base_url_rejects_non_rfc_or_disallowed_forms() {
    for base_url in [
        "http://example.com/attachments",
        "https://example.com/attachments?key=value",
        "https://example.com/attachments#section",
        "https://example.com/attachments/",
        "https://example.com/attachments/ ",
        "https://example.com/attachments/\n",
        "https://example.com/a/..",
        "https://example.com/x/../files",
        "https://example.com/./files",
        "https://example.com/%2e/files",
        " https://example.com/a",
        "https://example.com/files ",
        "https://example.com/fi les",
        "https://example.com/fi\tles",
        r"https:\\example.com\a",
        "https:example.com/a",
        "https:example.com/files",
        "https:/example.com/files",
        "https://bücher.example/att",
        "https://exa%6Dple.com/att",
        "http://2130706433/att",
        "http://0x7f.1/att",
        "http://127.1/att",
        "https://user:secret@example.com/attachments",
        "https://user@example.com/attachments",
    ] {
        let error = check_base_url(base_url).unwrap_err();
        assert_eq!(error.field(), "base_url", "{base_url}");
        assert!(!error.reason().is_empty(), "{base_url}");
    }
}

// verifies: ATCH-003, ATCH-004, ATCH-008
#[xmtp_common::test(unwrap_try = true)]
fn upload_and_retention_bounds() {
    for value in [
        1,
        BACKEND_DEFAULT_MAX_UPLOAD_BYTES,
        MAX_ATTACHMENT_UPLOAD_BYTES,
    ] {
        assert_eq!(check_max_upload_bytes(value), Ok(()));
    }
    for value in [0, MAX_ATTACHMENT_UPLOAD_BYTES + 1] {
        let error = check_max_upload_bytes(value).unwrap_err();
        assert_eq!(error.field(), "max_upload_bytes");
        assert!(!error.reason().is_empty());
    }
    for value in [0, MAX_ATTACHMENT_RETENTION_SECONDS] {
        assert_eq!(check_retention_seconds(value), Ok(()));
    }
    let error = check_retention_seconds(MAX_ATTACHMENT_RETENTION_SECONDS + 1).unwrap_err();
    assert_eq!(error.field(), "retention_seconds");
    assert!(!error.reason().is_empty());
}
