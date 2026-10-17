use super::*;
use crate::{
    db::PushChannel,
    push::channel::DeliveryConfig,
    test_support::{
        TestResult,
        push_provider::{Protocol, Provider, Reply},
    },
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::{
    ecdsa::{Signature, VerifyingKey, signature::Verifier},
    pkcs8::{EncodePrivateKey, LineEnding},
};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

pub(crate) fn config() -> ApnsConfig {
    let key = p256::SecretKey::from_slice(&[0x42; 32]).unwrap();
    ApnsConfig {
        key: Some(key.to_pkcs8_pem(LineEnding::LF).unwrap().to_string()),
        key_id: Some("ABC123DEFG".into()),
        team_id: Some("TEAM123456".into()),
        bundle_id: Some("org.example.app".into()),
        environment: Some("production".into()),
    }
}

pub(crate) fn sender(provider: &Provider) -> TestResult<ApnsSender> {
    let mut sender = ApnsSender::new(&config())?;
    let mut roots = rustls::RootCertStore::empty();
    roots.add(provider.root.clone())?;
    let tls = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = HttpsConnectorBuilder::new()
        .with_tls_config(tls)
        .https_only()
        .enable_http2()
        .build();
    sender.client = Client::builder(TokioExecutor::new())
        .http2_only(true)
        .build(connector);
    sender.endpoint = provider.endpoint.clone();
    Ok(sender)
}

pub(crate) fn delivery() -> Delivery {
    Delivery {
        payload: xmtp_push_types::PushPayload::new(&[1; 33], 9_007_199_254_740_993),
        config: DeliveryConfig {
            recipient_id: vec![0x8d; 32],
            secret_hash: vec![0x4a; 32],
            channel: PushChannel::Apns,
            delivery: "abcdef0123456789".repeat(4),
            signing_key: None,
        },
    }
}

/// Verify the JWS bytes with a separate signature verifier, not JWT decoding.
fn verify_token(value: &str) -> TestResult {
    let parts: Vec<_> = value.split('.').collect();
    assert_eq!(parts.len(), 3);
    for part in &parts {
        assert!(
            part.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        );
    }
    let header: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0])?)?;
    assert_eq!(header["alg"], "ES256");
    assert_eq!(header["kid"], "ABC123DEFG");
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1])?)?;
    assert_eq!(claims["iss"], "TEAM123456");
    assert!((claims["iat"].as_i64().unwrap() - xmtp_common::time::now_secs()).abs() <= 2);
    assert_eq!(claims.as_object().unwrap().len(), 2);
    let signature = URL_SAFE_NO_PAD.decode(parts[2])?;
    assert_eq!(signature.len(), 64, "JWS uses fixed-width R||S, never DER");
    let key = p256::SecretKey::from_slice(&[0x42; 32])?;
    VerifyingKey::from(key.public_key()).verify(
        format!("{}.{}", parts[0], parts[1]).as_bytes(),
        &Signature::from_slice(&signature)?,
    )?;
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn jwt_verifies_as_base64url_with_fixed_width_signature() {
    let sender = ApnsSender::new(&config())?;
    verify_token(&sender.token().await.unwrap())?;
}

// verifies: PUSH-259
#[xmtp_common::test(unwrap_try = true)]
async fn http2_request_has_only_background_payload_and_exact_headers() {
    let provider = Provider::start(Protocol::Http2Tls, vec![Reply::json(200, json!({}))]).await?;
    let sender = sender(&provider)?;
    let delivery = delivery();
    assert_eq!(sender.send(&delivery).await, Outcome::Delivered);
    let request = provider.requests.lock()[0].clone();
    assert_eq!(request.version, http::Version::HTTP_2);
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(
        request.uri.path(),
        format!("/3/device/{}", delivery.config.delivery)
    );
    assert_eq!(
        request.uri.authority().unwrap().as_str(),
        provider.endpoint.trim_start_matches("https://")
    );
    assert_eq!(request.headers["content-type"], "application/json");
    assert_eq!(request.headers["apns-topic"], "org.example.app");
    assert_eq!(request.headers["apns-push-type"], "background");
    assert_eq!(request.headers["apns-priority"], "5");
    assert_eq!(request.headers["apns-collapse-id"], delivery.payload.topic);
    assert_eq!(delivery.payload.topic.len(), 44);
    assert!(request.headers["apns-collapse-id"].len() <= 64);
    verify_token(
        request.headers["authorization"]
            .to_str()?
            .strip_prefix("bearer ")
            .unwrap(),
    )?;
    assert_eq!(
        serde_json::from_slice::<Value>(&request.body)?,
        json!({
            "aps": {"content-available": 1}, "topic": "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEB", "sequence_id": "9007199254740993"
        })
    );
    let mut production = config();
    assert_eq!(
        ApnsSender::new(&production)?.endpoint,
        "https://api.push.apple.com"
    );
    production.environment = Some("sandbox".into());
    assert_eq!(
        ApnsSender::new(&production)?.endpoint,
        "https://api.sandbox.push.apple.com"
    );
}

// verifies: PUSH-259
#[xmtp_common::test(unwrap_try = true)]
async fn status_and_reason_together_define_deletion_and_mismatch() {
    let transient = Outcome::Transient { retry_after: None };
    let cases = [
        (200, "", Outcome::Delivered),
        (201, "", Outcome::Rejected),
        (410, "Unregistered", Outcome::Terminal),
        (410, "ExpiredToken", Outcome::Terminal),
        (400, "BadDeviceToken", Outcome::Mismatch),
        (400, "DeviceTokenNotForTopic", Outcome::Mismatch),
        (400, "Unregistered", Outcome::Rejected),
        (410, "BadDeviceToken", Outcome::Rejected),
        (410, "Unknown", Outcome::Rejected),
        (400, "Unknown", Outcome::Rejected),
        (403, "InvalidProviderToken", Outcome::Rejected),
        (404, "BadPath", Outcome::Rejected),
        (405, "MethodNotAllowed", Outcome::Rejected),
        (413, "PayloadTooLarge", Outcome::Rejected),
        (429, "TooManyRequests", transient),
        (500, "Unknown", transient),
        (503, "Shutdown", transient),
        (502, "InternalServerError", Outcome::Rejected),
        (302, "", Outcome::Rejected),
    ];
    let provider = Provider::start(
        Protocol::Http2Tls,
        cases
            .iter()
            .map(|(status, reason, _)| Reply::json(*status, json!({"reason": reason})))
            .collect(),
    )
    .await?;
    let sender = sender(&provider)?;
    for (status, reason, expected) in cases {
        assert_eq!(
            sender.send(&delivery()).await,
            expected,
            "{status} {reason}"
        );
    }
    assert_eq!(provider.count(), cases.len());
}

#[xmtp_common::test(unwrap_try = true)]
async fn expired_token_refresh_is_shared_and_a_signing_failure_recovers() {
    let mut sender = ApnsSender::new(&config())?;
    sender.token.lock().await.as_mut().unwrap().issued -= REFRESH_INTERVAL;
    let tokens = futures::future::join_all((0..64).map(|_| sender.token())).await;
    assert!(
        tokens
            .iter()
            .all(|token| token.as_ref().unwrap() == tokens[0].as_ref().unwrap())
    );
    assert_eq!(sender.refreshes.load(Ordering::SeqCst), 1);
    let valid_key = std::mem::replace(
        &mut sender.key,
        EncodingKey::from_secret(b"invalid signing key"),
    );
    sender.token.lock().await.as_mut().unwrap().issued -= REFRESH_INTERVAL;
    let delivery = delivery();
    let results = futures::future::join_all((0..64).map(|_| sender.send(&delivery))).await;
    assert!(
        results
            .iter()
            .all(|result| *result == Outcome::Transient { retry_after: None })
    );
    assert_eq!(sender.refreshes.load(Ordering::SeqCst), 2);
    sender.key = valid_key;
    sender.token.lock().await.as_mut().unwrap().issued -= RETRY_DELAY;
    verify_token(&sender.token().await.unwrap())?;
    assert_eq!(sender.refreshes.load(Ordering::SeqCst), 3);
}

#[xmtp_common::test(unwrap_try = true)]
async fn provider_token_expiry_refreshes_once_and_repeated_expiry_keeps_recipient() {
    for (second, expected) in [(200, Outcome::Delivered), (403, Outcome::Rejected)] {
        let provider = Provider::start(
            Protocol::Http2Tls,
            vec![
                Reply::json(403, json!({"reason": "ExpiredProviderToken"})),
                Reply::json(second, json!({"reason": "ExpiredProviderToken"})),
            ],
        )
        .await?;
        let sender = sender(&provider)?;
        sender.token.lock().await.as_mut().unwrap().value = Ok("stale-credential".into());
        assert_eq!(sender.send(&delivery()).await, expected);
        assert_eq!(provider.count(), 2);
        assert_eq!(sender.refreshes.load(Ordering::SeqCst), 1);
        assert_eq!(sender.token.lock().await.is_none(), second == 403);
        let requests = provider.requests.lock();
        assert_eq!(
            requests[0].headers["authorization"],
            "bearer stale-credential"
        );
        let refreshed = requests[1].headers["authorization"]
            .to_str()?
            .strip_prefix("bearer ")
            .unwrap();
        verify_token(refreshed)?;
        assert_eq!(requests[0].body, requests[1].body);
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn stale_expiry_response_does_not_invalidate_a_newer_cached_token() {
    let provider = Provider::start(
        Protocol::Http2Tls,
        vec![Reply::json(403, json!({"reason": "ExpiredProviderToken"}))],
    )
    .await?;
    let sender = sender(&provider)?;
    let current = sender.token().await.unwrap();
    assert!(sender.request(&delivery(), "older-token").await.is_err());
    assert_eq!(sender.token().await.unwrap(), current);
    assert_eq!(sender.refreshes.load(Ordering::SeqCst), 0);
}

#[xmtp_common::test(unwrap_try = true)]
async fn credential_resend_uses_the_original_attempt_deadline() {
    let provider = Provider::start(
        Protocol::Http2Tls,
        vec![
            Reply {
                delay: Duration::from_secs(3),
                ..Reply::json(403, json!({"reason": "ExpiredProviderToken"}))
            },
            Reply {
                stall_headers: true,
                ..Reply::json(200, json!({}))
            },
        ],
    )
    .await?;
    let sender = sender(&provider)?;
    let started = Instant::now();
    let outcome = xmtp_common::time::timeout(
        ATTEMPT_TIMEOUT + Duration::from_secs(2),
        sender.send(&delivery()),
    )
    .await?;
    assert_eq!(outcome, Outcome::Transient { retry_after: None });
    assert!(started.elapsed() >= ATTEMPT_TIMEOUT);
    assert_eq!(provider.count(), 2);
}

#[xmtp_common::test(unwrap_try = true)]
async fn whole_attempt_deadline_covers_headers_body_and_credential_wait() {
    let headers = Provider::start(
        Protocol::Http2Tls,
        vec![Reply {
            stall_headers: true,
            ..Reply::json(200, json!({}))
        }],
    )
    .await?;
    let body = Provider::start(
        Protocol::Http2Tls,
        vec![Reply {
            stall_body: true,
            ..Reply::json(200, json!({}))
        }],
    )
    .await?;
    let header_sender = sender(&headers)?;
    let body_sender = sender(&body)?;
    let credential_sender = sender(&headers)?;
    let _credential_lock = credential_sender.token.lock().await;
    let delivery = delivery();
    let started = Instant::now();
    let results = xmtp_common::time::timeout(ATTEMPT_TIMEOUT + Duration::from_secs(2), async {
        tokio::join!(
            header_sender.send(&delivery),
            body_sender.send(&delivery),
            credential_sender.send(&delivery)
        )
    })
    .await?;
    assert_eq!(
        results,
        (
            Outcome::Transient { retry_after: None },
            Outcome::Transient { retry_after: None },
            Outcome::Transient { retry_after: None }
        )
    );
    assert!(started.elapsed() >= ATTEMPT_TIMEOUT);
    assert_eq!(headers.count(), 1);
    assert_eq!(body.count(), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn request_and_echoed_error_do_not_expose_private_fields_in_logs() {
    let delivery = delivery();
    let echo = format!(
        "{} {} {}",
        delivery.config.delivery,
        delivery.payload.topic,
        hex::encode(&delivery.config.recipient_id)
    );
    let provider = Provider::start(
        Protocol::Http2Tls,
        vec![Reply::json(400, json!({"reason": echo}))],
    )
    .await?;
    let sender = sender(&provider)?;
    let capture = xmtp_logging::test_logging::LogCapture::new(xmtp_logging::Level::Trace);
    let outcome = async {
        tracing::info!(target: "xmtp_backend::push", "provider privacy capture active");
        sender.send(&delivery).await
    }
    .with_subscriber(capture.dispatch())
    .await;
    assert_eq!(outcome, Outcome::Rejected);
    let output = capture.output();
    assert!(output.contains("provider privacy capture active"));
    for secret in [
        echo,
        delivery.config.delivery,
        delivery.payload.topic,
        hex::encode(delivery.config.recipient_id),
        config().key.unwrap(),
        provider.endpoint.clone(),
    ] {
        assert!(
            !output.contains(&secret),
            "provider logs contain private fields"
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn oversized_response_and_connection_failure_are_bounded_transient_failures() {
    let provider = Provider::start(
        Protocol::Http2Tls,
        vec![Reply {
            body: vec![b'x'; MAX_RESPONSE_BYTES + 1],
            ..Reply::json(400, json!({}))
        }],
    )
    .await?;
    let sender = sender(&provider)?;
    assert_eq!(
        sender.send(&delivery()).await,
        Outcome::Transient { retry_after: None }
    );
    assert_eq!(provider.count(), 1);
    drop(provider);
    assert_eq!(
        sender.send(&delivery()).await,
        Outcome::Transient { retry_after: None }
    );
}
