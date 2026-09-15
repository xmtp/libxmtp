use super::*;
use crate::{
    db::PushChannel,
    test_support::{
        TestResult,
        auth::TestKey,
        push_provider::{Protocol, Provider, Reply},
    },
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};

pub(crate) fn service_account() -> String {
    credentials(TOKEN_URI)
}

fn credentials(token_uri: &str) -> String {
    json!({
        "type": "service_account", "project_id": "example-project", "private_key": TestKey::rsa().private_key,
        "client_email": "push@example-project.iam.gserviceaccount.com", "token_uri": token_uri,
    }).to_string()
}

pub(crate) fn token_reply() -> Reply {
    Reply::json(
        200,
        json!({"access_token": "private-oauth-token", "token_type": "Bearer", "expires_in": 3600}),
    )
}

pub(crate) fn sender(token: &Provider, provider: &Provider) -> TestResult<FcmSender> {
    let mut sender = FcmSender::from_json(&credentials(&format!("{}/token", token.endpoint)))?;
    sender.endpoint = format!(
        "{}/v1/projects/example-project/messages:send",
        provider.endpoint
    );
    Ok(sender)
}

pub(crate) fn delivery() -> Delivery {
    let mut delivery = super::super::apns::tests::delivery();
    delivery.config.channel = PushChannel::Fcm;
    delivery.config.delivery = "private-fcm-registration-token".into();
    delivery
}

pub(crate) fn failure(code: &str) -> Value {
    json!({"error": {"code": 400, "status": "INVALID_ARGUMENT", "details": [
        {"@type": "type.googleapis.com/google.rpc.BadRequest", "fieldViolations": [{"field": "message.token"}]},
        {"@type": FCM_ERROR_TYPE, "errorCode": code}
    ]}})
}

fn expire(sender: &FcmSender) {
    let mut cache = sender.refresh.lock();
    let mut token = cache.as_ref().unwrap().peek().unwrap().clone();
    token.refresh_at = Instant::now() - Duration::from_secs(1);
    let flight = futures::future::ready(token).boxed().shared();
    assert!(flight.clone().now_or_never().is_some());
    *cache = Some(flight);
}

#[xmtp_common::test(unwrap_try = true)]
async fn requests_use_service_account_oauth_and_exact_data_only_message() {
    let token = Provider::start(Protocol::Http1, vec![token_reply()]).await?;
    let provider = Provider::start(
        Protocol::Http1,
        vec![Reply::json(200, json!({"name": "message"}))],
    )
    .await?;
    let sender = sender(&token, &provider)?;
    let delivery = delivery();
    assert_eq!(sender.send(&delivery).await, Outcome::Delivered);
    let request = token.requests.lock()[0].clone();
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(request.uri.path(), "/token");
    assert_eq!(
        request.headers["content-type"],
        "application/x-www-form-urlencoded"
    );
    let form: std::collections::HashMap<_, _> = url::form_urlencoded::parse(&request.body)
        .into_owned()
        .collect();
    assert_eq!(
        form["grant_type"],
        "urn:ietf:params:oauth:grant-type:jwt-bearer"
    );
    assert_eq!(form.len(), 2);
    let assertion = &form["assertion"];
    let parts: Vec<_> = assertion.split('.').collect();
    assert_eq!(parts.len(), 3);
    let key = jsonwebtoken::DecodingKey::from_rsa_pem(TestKey::rsa().public_key.as_bytes())?;
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_audience(&[format!("{}/token", token.endpoint)]);
    let claims = jsonwebtoken::decode::<Value>(assertion, &key, &validation)?.claims;
    assert_eq!(
        claims["iss"],
        "push@example-project.iam.gserviceaccount.com"
    );
    assert_eq!(
        claims["scope"],
        "https://www.googleapis.com/auth/firebase.messaging"
    );
    assert_eq!(
        claims["exp"].as_i64().unwrap() - claims["iat"].as_i64().unwrap(),
        3600
    );
    let header: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0])?)?;
    assert_eq!(header["alg"], "RS256");
    let request = provider.requests.lock()[0].clone();
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(
        request.uri.path(),
        "/v1/projects/example-project/messages:send"
    );
    assert_eq!(
        request.headers["host"],
        provider.endpoint.trim_start_matches("http://")
    );
    assert_eq!(request.headers["content-type"], "application/json");
    assert_eq!(
        request.headers["authorization"],
        "Bearer private-oauth-token"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&request.body)?,
        json!({"message": {
            "token": "private-fcm-registration-token",
            "data": {"topic": "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEB", "sequence_id": "9007199254740993"},
            "android": {"priority": "HIGH"},
            "apns": {"headers": {"apns-priority": "5", "apns-push-type": "background"}, "payload": {"aps": {"content-available": 1}}}
        }})
    );
    let production = FcmSender::new(&FcmConfig {
        service_account: Some(service_account()),
    })?;
    assert_eq!(
        production.endpoint,
        "https://fcm.googleapis.com/v1/projects/example-project/messages:send"
    );
    assert_eq!(
        production.account.token_uri,
        "https://oauth2.googleapis.com/token"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn only_typed_fcm_errors_can_delete_or_report_mismatch() {
    let cases = [
        (200, json!({}), Outcome::Delivered),
        (404, failure("UNREGISTERED"), Outcome::Terminal),
        (403, failure("SENDER_ID_MISMATCH"), Outcome::Mismatch),
        (400, failure("INVALID_ARGUMENT"), Outcome::Rejected),
        (401, failure("THIRD_PARTY_AUTH_ERROR"), Outcome::Rejected),
        (
            404,
            json!({"error": {"status": "UNREGISTERED"}}),
            Outcome::Rejected,
        ),
        (
            403,
            json!({"error": {"status": "SENDER_ID_MISMATCH"}}),
            Outcome::Rejected,
        ),
        (
            404,
            json!({"error": {"details": [{"@type": "google.rpc.ErrorInfo", "errorCode": "UNREGISTERED"}]}}),
            Outcome::Rejected,
        ),
        (404, failure("FUTURE_ERROR"), Outcome::Rejected),
        (
            503,
            json!({"error": {"status": "UNAVAILABLE"}}),
            Outcome::Rejected,
        ),
        (
            500,
            failure("INTERNAL"),
            Outcome::Transient { retry_after: None },
        ),
        (
            503,
            failure("UNAVAILABLE"),
            Outcome::Transient { retry_after: None },
        ),
        (302, json!({}), Outcome::Rejected),
    ];
    let token = Provider::start(Protocol::Http1, vec![token_reply()]).await?;
    let provider = Provider::start(
        Protocol::Http1,
        cases
            .iter()
            .map(|(status, body, _)| {
                let mut reply = Reply::json(*status, body.clone());
                reply.headers.push(("location", "/redirected".into()));
                reply
            })
            .collect(),
    )
    .await?;
    let sender = sender(&token, &provider)?;
    for (status, _, expected) in &cases {
        assert_eq!(sender.send(&delivery()).await, *expected, "HTTP {status}");
    }
    assert_eq!(
        provider.count(),
        cases.len(),
        "redirects must not cause another send"
    );
    assert_eq!(token.count(), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn quota_and_retry_after_delays_are_bounded_and_never_under_one_minute_for_quota() {
    let cases = [
        ("QUOTA_EXCEEDED", None, 60),
        ("QUOTA_EXCEEDED", Some("1"), 60),
        ("QUOTA_EXCEEDED", Some("120"), 120),
        ("QUOTA_EXCEEDED", Some("999"), 300),
        ("UNAVAILABLE", Some("42"), 42),
        ("UNAVAILABLE", Some("999"), 300),
        ("INTERNAL", Some("80"), 80),
    ];
    let token = Provider::start(Protocol::Http1, vec![token_reply()]).await?;
    let provider = Provider::start(
        Protocol::Http1,
        cases
            .iter()
            .map(|(code, delay, _)| {
                let mut reply = Reply::json(503, failure(code));
                if let Some(delay) = delay {
                    reply.headers.push(("retry-after", delay.to_string()));
                }
                reply
            })
            .collect(),
    )
    .await?;
    let sender = sender(&token, &provider)?;
    for (_, _, seconds) in cases {
        assert_eq!(
            sender.send(&delivery()).await,
            Outcome::Transient {
                retry_after: Some(Duration::from_secs(seconds))
            }
        );
    }
    let date = httpdate::fmt_http_date(std::time::SystemTime::now() + Duration::from_secs(120));
    let delay = retry_after(&date).unwrap();
    assert!(delay >= Duration::from_secs(118) && delay <= Duration::from_secs(120));
    assert_eq!(retry_after("not a delay"), None);
    assert_eq!(retry_after("-1"), None);
    assert_eq!(
        retry_after("Wed, 01 Jan 2020 00:00:00 GMT"),
        Some(Duration::ZERO)
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_cache_misses_and_expiry_share_one_exchange_and_failures_recover() {
    let mut reply = token_reply();
    reply.delay = Duration::from_millis(50);
    let token = Provider::start(Protocol::Http1, vec![reply.clone()]).await?;
    let provider = Provider::start(Protocol::Http1, vec![Reply::json(200, json!({}))]).await?;
    let sender = sender(&token, &provider)?;
    for expected_exchanges in 1..=2 {
        let results = futures::future::join_all((0..64).map(|_| sender.token())).await;
        assert!(
            results
                .iter()
                .all(|result| result.as_ref().unwrap().as_ref() == "private-oauth-token")
        );
        assert_eq!(token.count(), expected_exchanges);
        expire(&sender);
    }
    token.set_replies(vec![Reply {
        delay: Duration::from_millis(50),
        ..Reply::json(500, json!({"error": "temporary"}))
    }]);
    let delivery = delivery();
    let results = futures::future::join_all((0..64).map(|_| sender.send(&delivery))).await;
    assert!(
        results
            .iter()
            .all(|result| *result == Outcome::Transient { retry_after: None })
    );
    assert_eq!(token.count(), 3);
    assert_eq!(provider.count(), 0);
    expire(&sender);
    token.set_replies(vec![reply]);
    assert_eq!(sender.send(&delivery).await, Outcome::Delivered);
    assert_eq!(token.count(), 4);
}

#[xmtp_common::test(unwrap_try = true)]
async fn whole_attempt_deadline_covers_provider_headers_body_and_oauth_headers_body() {
    let token = Provider::start(
        Protocol::Http1,
        vec![Reply {
            delay: Duration::from_secs(3),
            ..token_reply()
        }],
    )
    .await?;
    let headers = Provider::start(
        Protocol::Http1,
        vec![Reply {
            stall_headers: true,
            ..Reply::json(200, json!({}))
        }],
    )
    .await?;
    let body = Provider::start(
        Protocol::Http1,
        vec![Reply {
            stall_body: true,
            ..Reply::json(200, json!({}))
        }],
    )
    .await?;
    let oauth_headers = Provider::start(
        Protocol::Http1,
        vec![Reply {
            stall_headers: true,
            ..token_reply()
        }],
    )
    .await?;
    let oauth_body = Provider::start(
        Protocol::Http1,
        vec![Reply {
            stall_body: true,
            ..token_reply()
        }],
    )
    .await?;
    let senders = [
        sender(&token, &headers)?,
        sender(&token, &body)?,
        sender(&oauth_headers, &headers)?,
        sender(&oauth_body, &headers)?,
    ];
    let delivery = delivery();
    let started = Instant::now();
    let results = xmtp_common::time::timeout(
        ATTEMPT_TIMEOUT + Duration::from_secs(2),
        futures::future::join_all(senders.iter().map(|sender| sender.send(&delivery))),
    )
    .await?;
    assert!(
        results
            .iter()
            .all(|outcome| *outcome == Outcome::Transient { retry_after: None })
    );
    assert!(started.elapsed() >= ATTEMPT_TIMEOUT - Duration::from_millis(100));
    assert_eq!(
        headers.count(),
        1,
        "stalled credentials must not issue a send"
    );
    assert_eq!(body.count(), 1);
    assert_eq!(oauth_headers.count(), 1);
    assert_eq!(oauth_body.count(), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn oauth_and_send_responses_have_a_byte_bound() {
    let oversized = Reply {
        body: vec![b'x'; MAX_RESPONSE_BYTES + 1],
        ..token_reply()
    };
    let token = Provider::start(Protocol::Http1, vec![oversized.clone()]).await?;
    let provider = Provider::start(Protocol::Http1, vec![oversized]).await?;
    let sender = sender(&token, &provider)?;
    assert_eq!(
        sender.send(&delivery()).await,
        Outcome::Transient { retry_after: None }
    );
    assert_eq!(provider.count(), 0);
    expire(&sender);
    token.set_replies(vec![token_reply()]);
    assert_eq!(
        sender.send(&delivery()).await,
        Outcome::Transient { retry_after: None }
    );
    assert_eq!(provider.count(), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn credentials_and_echoed_provider_errors_do_not_expose_private_fields() {
    let delivery = delivery();
    let echo = format!(
        "{} {} {} private-provider-metadata private-oauth-token",
        delivery.config.delivery,
        delivery.payload.topic,
        hex::encode(&delivery.config.recipient_id)
    );
    let token = Provider::start(
        Protocol::Http1,
        vec![Reply::json(500, json!({"error": echo}))],
    )
    .await?;
    let provider = Provider::start(
        Protocol::Http1,
        vec![Reply::json(400, json!({"error": echo}))],
    )
    .await?;
    let sender = sender(&token, &provider)?;
    let capture = xmtp_logging::test_logging::LogCapture::new(xmtp_logging::Level::Trace);
    async {
        tracing::info!(target: "xmtp_backend::push", "provider privacy capture active");
        assert_eq!(
            sender.send(&delivery).await,
            Outcome::Transient { retry_after: None }
        );
        expire(&sender);
        token.set_replies(vec![token_reply()]);
        assert_eq!(sender.send(&delivery).await, Outcome::Rejected);
        let error = FcmSender::new(&FcmConfig {
            service_account: Some(echo.clone()),
        })
        .err()
        .unwrap();
        tracing::warn!(target: "xmtp_backend::push", %error, "provider config is invalid");
    }
    .with_subscriber(capture.dispatch())
    .await;
    let output = capture.output();
    assert!(output.contains("provider privacy capture active"));
    assert!(output.contains("provider config is invalid"));
    for secret in [
        echo,
        delivery.config.delivery,
        delivery.payload.topic,
        hex::encode(delivery.config.recipient_id),
        TestKey::rsa().private_key,
        token.endpoint.clone(),
        provider.endpoint.clone(),
        "private-oauth-token".into(),
    ] {
        assert!(
            !output.contains(&secret),
            "provider logs contain private fields"
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn startup_rejects_invalid_credentials_and_endpoint_overrides_without_values() {
    for changes in [
        json!({"project_id": ""}),
        json!({"project_id": "project/other"}),
        json!({"private_key": "private-invalid-key"}),
        json!({"token_uri": "http://127.0.0.1/token"}),
        json!({"client_email": ""}),
    ] {
        let mut value: Value = serde_json::from_str(&service_account())?;
        for (key, value_override) in changes.as_object().unwrap() {
            value[key] = value_override.clone();
        }
        let error = FcmSender::new(&FcmConfig {
            service_account: Some(value.to_string()),
        })
        .err()
        .unwrap();
        assert_eq!(
            error.to_string(),
            "configuration is invalid: push.fcm.service_account (must contain a valid service account, project_id, and Google token_uri)"
        );
        assert!(!format!("{error:?}").contains("private-invalid-key"));
    }
}
