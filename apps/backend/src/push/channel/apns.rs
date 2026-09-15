//! APNs background pushes over HTTP/2 with ES256 provider credentials.

use super::{ATTEMPT_TIMEOUT, Delivery, MAX_RESPONSE_BYTES, Outcome, RETRY_DELAY, Sender};
use crate::config::{ConfigError, push::ApnsConfig};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::instrument::WithSubscriber;
use xmtp_common::time::{Duration, Instant};

const PRODUCTION: &str = "https://api.push.apple.com";
const SANDBOX: &str = "https://api.sandbox.push.apple.com";
const REFRESH_INTERVAL: Duration = Duration::from_secs(55 * 60);
type HttpClient = Client<HttpsConnector<HttpConnector>, Full<Bytes>>;

pub(crate) struct ApnsSender {
    client: HttpClient,
    endpoint: String,
    bundle_id: String,
    key: EncodingKey,
    header: Header,
    team_id: String,
    token: Mutex<Option<CachedToken>>,
    #[cfg(test)]
    refreshes: std::sync::atomic::AtomicUsize,
}

struct CachedToken {
    value: Result<String, ()>,
    issued: Instant,
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    iat: i64,
}

impl ApnsSender {
    /// Validate the signing key at startup. Endpoints cannot be set by config.
    pub fn new(config: &ApnsConfig) -> Result<Self, ConfigError> {
        let invalid = || ConfigError::Invalid {
            field: "push.apns.key",
            reason: "must be a PKCS#8 ES256 private key",
        };
        let key = EncodingKey::from_ec_pem(config.key.as_deref().unwrap_or_default().as_bytes())
            .map_err(|_| invalid())?;
        let mut header = Header::new(Algorithm::ES256);
        header.kid = config.key_id.clone();
        let team_id = config.team_id.clone().unwrap_or_default();
        // Signing also rejects other EC curves accepted by the PEM parser.
        let value = jsonwebtoken::encode(
            &header,
            &Claims {
                iss: &team_id,
                iat: xmtp_common::time::now_secs(),
            },
            &key,
        )
        .map_err(|_| invalid())?;
        let connector = HttpsConnectorBuilder::new()
            .with_provider_and_webpki_roots(std::sync::Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .map_err(|_| ConfigError::Invalid {
                field: "push.apns",
                reason: "TLS initialization failed",
            })?
            .https_only()
            .enable_http2()
            .build();
        Ok(Self {
            client: Client::builder(TokioExecutor::new())
                .http2_only(true)
                .build(connector),
            endpoint: if config.environment.as_deref() == Some("sandbox") {
                SANDBOX
            } else {
                PRODUCTION
            }
            .into(),
            bundle_id: config.bundle_id.clone().unwrap_or_default(),
            key,
            header,
            team_id,
            token: Mutex::new(Some(CachedToken {
                value: Ok(value),
                issued: Instant::now(),
            })),
            #[cfg(test)]
            refreshes: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// Hold the lock through signing so concurrent expiry observes one token.
    /// The attempt deadline also bounds waiting for this lock.
    async fn token(&self) -> Result<String, ()> {
        let mut cached = self.token.lock().await;
        if let Some(token) = cached.as_ref().filter(|token| {
            token.issued.elapsed()
                < if token.value.is_ok() {
                    REFRESH_INTERVAL
                } else {
                    RETRY_DELAY
                }
        }) {
            return token.value.clone();
        }
        // A failed refresh is shared until the normal retry delay. A burst of
        // waiters must not each repeat the same failed signing operation.
        #[cfg(test)]
        self.refreshes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let value = jsonwebtoken::encode(
            &self.header,
            &Claims {
                iss: &self.team_id,
                iat: xmtp_common::time::now_secs(),
            },
            &self.key,
        )
        .map_err(|_| ());
        *cached = Some(CachedToken {
            value: value.clone(),
            issued: Instant::now(),
        });
        value
    }

    /// Read a bounded response body before classifying the provider answer.
    async fn attempt(&self, delivery: &Delivery) -> Outcome {
        let Ok(token) = self.token().await else {
            return Outcome::Transient { retry_after: None };
        };
        if delivery.config.delivery.is_empty()
            || !delivery
                .config
                .delivery
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Outcome::Rejected;
        }
        let body = serde_json::json!({
            "aps": {"content-available": 1},
            "topic": delivery.payload.topic,
            "sequence_id": delivery.payload.sequence_id,
        });
        let mut authorization = match http::HeaderValue::from_str(&format!("bearer {token}")) {
            Ok(value) => value,
            Err(_) => return Outcome::Rejected,
        };
        authorization.set_sensitive(true);
        let request = http::Request::post(format!(
            "{}/3/device/{}",
            self.endpoint, delivery.config.delivery
        ))
        .version(http::Version::HTTP_2)
        .header(http::header::AUTHORIZATION, authorization)
        .header(http::header::CONTENT_TYPE, "application/json")
        .header("apns-topic", &self.bundle_id)
        .header("apns-push-type", "background")
        .header("apns-priority", "5")
        .header("apns-collapse-id", &delivery.payload.topic)
        .body(Full::new(Bytes::from(body.to_string())));
        let Ok(request) = request else {
            return Outcome::Rejected;
        };
        let Ok(response) = self.client.request(request).await else {
            return Outcome::Transient { retry_after: None };
        };
        let status = response.status().as_u16();
        let Ok(body) = Limited::new(response.into_body(), MAX_RESPONSE_BYTES)
            .collect()
            .await
        else {
            return Outcome::Transient { retry_after: None };
        };
        #[derive(Deserialize)]
        struct Failure {
            reason: String,
        }
        let reason = serde_json::from_slice::<Failure>(&body.to_bytes()).ok();
        classify(status, reason.as_ref().map(|error| error.reason.as_str()))
    }
}

#[async_trait::async_trait]
impl Sender for ApnsSender {
    async fn send(&self, delivery: &Delivery) -> Outcome {
        // HTTP tracing can include a device token in the URI. Keep all transport
        // diagnostics outside the application's subscriber.
        xmtp_common::time::timeout(ATTEMPT_TIMEOUT, self.attempt(delivery))
            .with_subscriber(tracing::Dispatch::none())
            .await
            .unwrap_or(Outcome::Transient { retry_after: None })
    }
}

fn classify(status: u16, reason: Option<&str>) -> Outcome {
    match (status, reason) {
        (200, _) => Outcome::Delivered,
        (410, Some("Unregistered" | "ExpiredToken")) => Outcome::Terminal,
        (400, Some("BadDeviceToken" | "DeviceTokenNotForTopic")) => Outcome::Mismatch,
        (429 | 500 | 503, _) => Outcome::Transient { retry_after: None },
        _ => Outcome::Rejected,
    }
}

#[cfg(test)]
pub(crate) mod tests;
