use crate::{
    api,
    test_support::{TestResult, TestServer, auth::api_key},
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tonic::{Code, Request};
use xmtp_attachments_server::{
    AttachmentsConfig, CredentialsConfig, PresignedRequest, S3Config, S3Target, SignError,
    StorageTarget, TargetConfig,
};

struct FailingTarget;

#[tonic::async_trait]
impl StorageTarget for FailingTarget {
    async fn presign_put(
        &self,
        _content_digest: &[u8; 32],
        _content_length: u64,
    ) -> Result<PresignedRequest, SignError> {
        Err(SignError::CredentialsUnavailable)
    }
}

fn attachment_config() -> AttachmentsConfig {
    let endpoint = std::env::var("XMTP_S3_URL").unwrap_or_else(|_| "http://127.0.0.1:9067".into());
    AttachmentsConfig {
        base_url: format!("{endpoint}/attachments"),
        max_upload_bytes: Some(1024),
        retention_seconds: Some(3600),
        target: TargetConfig::S3(S3Config {
            endpoint,
            region: "us-east-1".into(),
            bucket: "attachments".into(),
            key_prefix: String::new(),
            credentials: CredentialsConfig::Static {
                access_key_id: "xmtps3".into(),
                secret_access_key: "xmtps3secret".into(),
                session_token: None,
            },
            presign_ttl_seconds: Some(300),
        }),
    }
}

fn request(digest: Vec<u8>, length: u64) -> api::CreateUploadRequest {
    api::CreateUploadRequest {
        content_digest: digest,
        content_length: length,
    }
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: ATCH-001, ATCH-004, ATCH-006
async fn offer_published_iff_configured() -> TestResult {
    let absent = TestServer::new(|_| {}).await?;
    assert!(
        absent
            .configuration()
            .get_configuration(api::GetConfigurationRequest {})
            .await?
            .into_inner()
            .attachments
            .is_none()
    );
    absent.stop().await?;
    let configured =
        TestServer::new(|config| config.attachments = Some(attachment_config())).await?;
    let published = configured
        .configuration()
        .get_configuration(api::GetConfigurationRequest {})
        .await?
        .into_inner();
    let offer = published.attachments.expect("configured offer");
    assert_eq!(offer.base_url, attachment_config().base_url);
    assert_eq!(offer.max_upload_bytes, 1024);
    assert_eq!(offer.retention_seconds, 3600);
    let wire = String::from_utf8_lossy(&offer.encode_to_vec()).into_owned();
    assert!(!wire.contains("xmtps3secret"));
    assert!(!wire.contains("xmtps3"));
    configured.stop().await
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: ATCH-004
async fn retention_published() -> TestResult {
    let server = TestServer::new(|config| {
        let mut attachments = attachment_config();
        attachments.retention_seconds = None;
        config.attachments = Some(attachments);
    })
    .await?;
    let offer = server
        .configuration()
        .get_configuration(api::GetConfigurationRequest {})
        .await?
        .into_inner()
        .attachments
        .expect("offer");
    assert_eq!(offer.retention_seconds, 0);
    server.stop().await
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: ATCH-006
async fn no_storage_secrets_published() -> TestResult {
    let server = TestServer::new(|config| config.attachments = Some(attachment_config())).await?;
    let response = server
        .configuration()
        .get_configuration(api::GetConfigurationRequest {})
        .await?
        .into_inner();
    let json = String::from_utf8_lossy(&response.encode_to_vec()).into_owned();
    for secret in [
        "xmtps3secret",
        "xmtps3",
        "access_key_id",
        "secret_access_key",
        "endpoint",
        "bucket",
        "key_prefix",
    ] {
        assert!(!json.contains(secret), "published secret field: {secret}");
    }
    server.stop().await
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: ATCH-020, ATCH-028
async fn create_upload_wire_round_trip() -> TestResult {
    let server = TestServer::new(|config| config.attachments = Some(attachment_config())).await?;
    let response = server
        .attachments()
        .create_upload(request(vec![7; 32], 42))
        .await?
        .into_inner();
    assert_eq!(response.method, "PUT");
    assert!(response.url.contains("/attachments/"));
    assert!((300..=3600).contains(&response.expires_in_seconds));
    assert_eq!(response.headers.len(), 3);
    assert!(
        response
            .headers
            .iter()
            .any(|header| header.name == "content-length" && header.value == "42")
    );
    assert!(
        response
            .headers
            .iter()
            .any(|header| header.name == "if-none-match" && header.value == "*")
    );
    assert!(
        response
            .headers
            .iter()
            .any(|header| header.name == "x-amz-checksum-sha256")
    );
    server.stop().await
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: ATCH-028
async fn expires_in_bounds() -> TestResult {
    let server = TestServer::new(|config| {
        let mut attachments = attachment_config();
        let TargetConfig::S3(s3) = &mut attachments.target;
        s3.presign_ttl_seconds = Some(3600);
        config.attachments = Some(attachments);
    })
    .await?;
    let signed = server
        .attachments()
        .create_upload(request(vec![9; 32], 9))
        .await?
        .into_inner();
    assert_eq!(signed.expires_in_seconds, 3600);
    server.stop().await
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: ATCH-021
async fn admission_table_order() -> TestResult {
    let absent = TestServer::new(|_| {}).await?;
    assert_eq!(
        absent
            .attachments()
            .create_upload(request(vec![], 0))
            .await
            .unwrap_err()
            .code(),
        Code::Unimplemented
    );
    absent.stop().await?;
    let configured =
        TestServer::new(|config| config.attachments = Some(attachment_config())).await?;
    for (digest, length) in [(vec![], 0), (vec![0; 31], 42), (vec![0; 33], 42)] {
        assert_eq!(
            configured
                .attachments()
                .create_upload(request(digest, length))
                .await
                .unwrap_err()
                .code(),
            Code::InvalidArgument
        );
    }
    for length in [0, 1025] {
        assert_eq!(
            configured
                .attachments()
                .create_upload(request(vec![0; 32], length))
                .await
                .unwrap_err()
                .code(),
            Code::InvalidArgument
        );
    }
    let mut failing_backend = configured.backend.clone();
    failing_backend.attachments = Some(Arc::new(FailingTarget));
    for length in [0, 1025] {
        let status = api::attachment_service_server::AttachmentService::create_upload(
            &failing_backend,
            Request::new(request(vec![0; 32], length)),
        )
        .await
        .unwrap_err();
        assert_eq!(status.code(), Code::InvalidArgument);
    }
    let status = api::attachment_service_server::AttachmentService::create_upload(
        &failing_backend,
        Request::new(request(vec![0; 32], 1)),
    )
    .await
    .unwrap_err();
    assert_eq!(status.code(), Code::Unavailable);
    let status: tonic::Status =
        crate::error::Error::from(xmtp_attachments_server::SignError::CredentialsUnavailable)
            .into();
    assert_eq!(status.code(), Code::Unavailable);
    let status: tonic::Status =
        crate::error::Error::from(xmtp_attachments_server::SignError::SigningFailed).into();
    assert_eq!(status.code(), Code::Internal);
    configured.stop().await
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: ATCH-021
async fn create_upload_requires_credential() -> TestResult {
    let (key, auth) = api_key("uploader");
    let server = TestServer::new(|config| {
        config.attachments = Some(attachment_config());
        config.auth = Some(auth);
    })
    .await?;
    let input = request(vec![3; 32], 3);
    assert_eq!(
        server
            .attachments()
            .create_upload(input.clone())
            .await
            .unwrap_err()
            .code(),
        Code::Unauthenticated
    );
    let mut authenticated = Request::new(input);
    authenticated
        .metadata_mut()
        .insert("authorization", format!("Bearer {key}").parse()?);
    assert_eq!(
        server
            .attachments()
            .create_upload(authenticated)
            .await?
            .into_inner()
            .method,
        "PUT"
    );
    server.stop().await
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: ATCH-023
async fn s3_target_accepts_exact_bytes_once() -> TestResult {
    let server = TestServer::new(|config| config.attachments = Some(attachment_config())).await?;
    let bytes = uuid::Uuid::new_v4().as_bytes().to_vec();
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    let signed = server
        .attachments()
        .create_upload(request(digest.to_vec(), bytes.len() as u64))
        .await?
        .into_inner();
    let client = xmtp_common::http::client()?;
    let preflight = client
        .request(reqwest::Method::OPTIONS, &signed.url)
        .header("origin", "http://example.test")
        .header("access-control-request-method", "PUT")
        .header(
            "access-control-request-headers",
            "content-length,host,if-none-match,x-amz-checksum-sha256",
        )
        .send()
        .await?;
    assert!(preflight.status().is_success());
    let allowed = preflight
        .headers()
        .get("access-control-allow-headers")
        .expect("CORS headers")
        .to_str()?
        .to_ascii_lowercase();
    for name in [
        "content-length",
        "host",
        "if-none-match",
        "x-amz-checksum-sha256",
    ] {
        assert!(
            allowed == "*" || allowed.contains(name),
            "CORS excludes {name}: {allowed}"
        );
    }
    assert!(
        preflight
            .headers()
            .contains_key("access-control-allow-origin")
    );
    assert!(
        preflight
            .headers()
            .get("access-control-allow-methods")
            .expect("CORS methods")
            .to_str()?
            .contains("PUT")
    );
    let put = |body: Vec<u8>| {
        let mut upload = client.put(&signed.url);
        for header in &signed.headers {
            upload = upload.header(header.name.as_str(), header.value.as_str());
        }
        upload.body(body)
    };
    let wrong_body = put(vec![0; bytes.len()]).send().await?;
    assert_eq!(wrong_body.status(), reqwest::StatusCode::BAD_REQUEST);
    let mut wrong_length = client.put(&signed.url);
    for header in &signed.headers {
        if header.name != "content-length" {
            wrong_length = wrong_length.header(header.name.as_str(), header.value.as_str());
        }
    }
    let wrong_length = wrong_length
        .body(bytes[..bytes.len() - 1].to_vec())
        .send()
        .await?;
    assert!(matches!(wrong_length.status().as_u16(), 400 | 403));
    let uploaded = put(bytes.clone()).send().await?;
    assert!(
        uploaded.status().is_success(),
        "{}: {}",
        uploaded.status(),
        uploaded.text().await?
    );
    let get_url = format!("{}/{}", attachment_config().base_url, hex::encode(digest));
    let get_preflight = client
        .request(reqwest::Method::OPTIONS, &get_url)
        .header("origin", "http://example.test")
        .header("access-control-request-method", "GET")
        .send()
        .await?;
    assert!(get_preflight.status().is_success());
    assert!(
        get_preflight
            .headers()
            .contains_key("access-control-allow-origin")
    );
    assert!(
        get_preflight
            .headers()
            .get("access-control-allow-methods")
            .expect("CORS methods")
            .to_str()?
            .contains("GET")
    );
    let fetched = client
        .get(get_url)
        .header("origin", "http://example.test")
        .send()
        .await?;
    assert_eq!(fetched.status(), reqwest::StatusCode::OK);
    assert!(
        fetched
            .headers()
            .contains_key("access-control-allow-origin")
    );
    assert_eq!(fetched.bytes().await?.as_ref(), bytes);
    assert_eq!(
        put(bytes.clone()).send().await?.status(),
        reqwest::StatusCode::PRECONDITION_FAILED
    );
    let past = Arc::new(|| SystemTime::now() - Duration::from_secs(3600));
    let expired_config = attachment_config();
    let TargetConfig::S3(s3) = &expired_config.target;
    let expired_target = S3Target::new_with_clock(s3, past).await?;
    let expired_signed = expired_target
        .presign_put(&digest, bytes.len() as u64)
        .await?;
    let expired = url::Url::parse(&expired_signed.url)?;
    // Send the full request in one write. A target can reject the date before
    // a streaming client sends its body and close that connection.
    let host = expired.host_str().expect("storage host");
    let port = expired.port_or_known_default().expect("storage port");
    let mut stream = tokio::net::TcpStream::connect((host, port)).await?;
    let mut wire = format!(
        "PUT {}?{} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n",
        expired.path(),
        expired.query().expect("signature query")
    )
    .into_bytes();
    for (name, value) in &expired_signed.headers {
        wire.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    wire.extend_from_slice(b"\r\n");
    wire.extend_from_slice(&bytes);
    stream.write_all(&wire).await?;
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).await?;
    let reply = String::from_utf8(reply)?;
    assert!(reply.starts_with("HTTP/1.1 403"), "{reply}");
    assert!(reply.to_ascii_lowercase().contains("expir"), "{reply}");
    server.stop().await
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: OPS-019
fn attachment_route_has_fixed_labels() {
    let labels =
        crate::telemetry::RpcLabels::from_path("/xmtp.backend.v1.AttachmentService/CreateUpload");
    assert_eq!(labels.service, "xmtp.backend.v1.AttachmentService");
    assert_eq!(labels.method, "CreateUpload");
    assert!(!labels.health);
}
