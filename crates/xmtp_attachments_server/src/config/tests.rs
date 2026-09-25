use super::*;

fn config() -> AttachmentsConfig {
    AttachmentsConfig {
        base_url: "https://attachments.example.com/objects".into(),
        max_upload_bytes: None,
        retention_seconds: None,
        target: TargetConfig::S3(S3Config {
            endpoint: "https://s3.example.com".into(),
            region: "us-east-1".into(),
            bucket: "attachments".into(),
            key_prefix: String::new(),
            credentials: CredentialsConfig::Environment,
            presign_ttl_seconds: None,
        }),
    }
}

// verifies: ATCH-002
#[xmtp_common::test(unwrap_try = true)]
async fn base_url_rules() {
    let mut config = config();
    for value in [
        "https://example.com/files",
        "http://localhost:9000/files",
        "http://127.0.0.1:9000/files",
        "http://[::1]:9000/files",
    ] {
        config.base_url = value.into();
        assert!(config.validate().is_ok(), "{value}");
    }
    for value in [
        "http://example.com/files",
        "http://10.0.0.1/files",
        "https://example.com/files?x=1",
        "https://example.com/files#x",
        "https://example.com/files/",
        "files",
        "file:///tmp/files",
        "https://user:pass@example.com/files",
        " https://example.com/files",
        "https://example.com/files ",
        "https://example.com/fi\tles",
        "https://example.com/my files",
        "https://example.com\\files",
        "https:example.com/files",
        "https:/example.com/files",
        "https://example.com/a/../files",
        "https://example.com/%2e/files",
        "HTTPS://example.com/files",
    ] {
        config.base_url = value.into();
        assert_eq!(config.validate().unwrap_err().field, "attachments.base_url");
    }
}

// verifies: ATCH-004
#[xmtp_common::test(unwrap_try = true)]
async fn retention_rules() {
    let mut config = config();
    assert!(config.validate().is_ok());
    for value in [0, xmtp_configuration::MAX_ATTACHMENT_RETENTION_SECONDS] {
        config.retention_seconds = Some(value);
        assert!(config.validate().is_ok());
    }
    config.retention_seconds = Some(xmtp_configuration::MAX_ATTACHMENT_RETENTION_SECONDS + 1);
    assert_eq!(
        config.validate().unwrap_err().field,
        "attachments.retention_seconds"
    );
}

// verifies: ATCH-075
#[xmtp_common::test(unwrap_try = true)]
async fn key_prefix_rules() {
    let mut config = config();
    for value in ["", "a/", "a/b/", "Ab_09-x.y/z/"] {
        let TargetConfig::S3(s3) = &mut config.target;
        s3.key_prefix = value.into();
        assert!(config.validate().is_ok(), "{value}");
    }
    for value in [
        "/lead/", "a//b/", "a/./b", "a/../b", "a b/", "a%2Fb/", "a+b/", "é/", "a\\b/",
    ] {
        let TargetConfig::S3(s3) = &mut config.target;
        s3.key_prefix = value.into();
        assert_eq!(
            config.validate().unwrap_err().field,
            "attachments.target.S3.key_prefix"
        );
    }
}

// verifies: ATCH-071 (backend)
#[xmtp_common::test(unwrap_try = true)]
async fn storage_endpoint_requires_https_or_loopback_http() {
    let mut config = config();
    for endpoint in [
        "https://s3.example.com",
        "http://127.0.0.1:9000",
        "http://localhost:9000",
    ] {
        let TargetConfig::S3(s3) = &mut config.target;
        s3.endpoint = endpoint.into();
        assert!(config.validate().is_ok(), "{endpoint}");
    }
    let TargetConfig::S3(s3) = &mut config.target;
    s3.endpoint = "http://s3.example.com".into();
    assert_eq!(
        config.validate().unwrap_err().field,
        "attachments.target.S3.endpoint"
    );
}

// verifies: ATCH-003
#[xmtp_common::test(unwrap_try = true)]
async fn upload_ceiling_rules() {
    let mut config = config();
    assert_eq!(config.upload_ceiling(), BACKEND_DEFAULT_MAX_UPLOAD_BYTES);
    assert!(config.validate().is_ok());
    for value in [1, xmtp_configuration::MAX_ATTACHMENT_UPLOAD_BYTES] {
        config.max_upload_bytes = Some(value);
        assert!(config.validate().is_ok());
        assert_eq!(config.upload_ceiling(), value);
    }
    for value in [0, xmtp_configuration::MAX_ATTACHMENT_UPLOAD_BYTES + 1] {
        config.max_upload_bytes = Some(value);
        assert_eq!(
            config.validate().unwrap_err().field,
            "attachments.max_upload_bytes"
        );
    }
}
