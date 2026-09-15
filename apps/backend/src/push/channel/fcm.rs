//! FCM data messages with explicit service-account credentials.

use super::{
    ATTEMPT_TIMEOUT, Delivery, MAX_RESPONSE_BYTES, MAX_RETRY_DELAY, Outcome, RETRY_DELAY, Sender,
};
use crate::config::{ConfigError, push::FcmConfig};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use futures::future::{BoxFuture, FutureExt, Shared};
use parking_lot::Mutex;
use ring::{
    rand::SystemRandom,
    signature::{RSA_PKCS1_SHA256, RsaKeyPair},
};
use rustls::pki_types::{PrivatePkcs8KeyDer, pem::PemObject};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::instrument::WithSubscriber;
use xmtp_common::time::{Duration, Instant};

const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const SEND_BASE: &str = "https://fcm.googleapis.com/v1/projects";
const SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";
const FCM_ERROR_TYPE: &str = "type.googleapis.com/google.firebase.fcm.v1.FcmError";
const QUOTA_DELAY: Duration = Duration::from_secs(60);
const TOKEN_LIFETIME_SECONDS: i64 = 3600;
const TOKEN_REFRESH_MARGIN: Duration = Duration::from_secs(20);
type TokenFlight = Shared<BoxFuture<'static, CachedToken>>;

struct ServiceAccount {
    key: RsaKeyPair,
    email: String,
    token_uri: String,
}

#[derive(Clone)]
struct CachedToken {
    value: Result<Arc<str>, ()>,
    refresh_at: Instant,
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'static str,
    aud: &'a str,
    iat: i64,
    exp: i64,
}

pub(crate) struct FcmSender {
    account: Arc<ServiceAccount>,
    refresh: Mutex<Option<TokenFlight>>,
    client: reqwest::Client,
    endpoint: String,
}

impl FcmSender {
    /// Load only the configured JSON credentials. Never discover ambient accounts.
    pub fn new(config: &FcmConfig) -> Result<Self, ConfigError> {
        let json = config.service_account.as_deref().unwrap_or_default();
        let value: serde_json::Value = serde_json::from_str(json).map_err(|_| invalid())?;
        if value.get("token_uri").and_then(|uri| uri.as_str()) != Some(TOKEN_URI) {
            return Err(invalid());
        }
        Self::from_json(json)
    }

    fn from_json(json: &str) -> Result<Self, ConfigError> {
        #[derive(Deserialize)]
        struct Credentials {
            project_id: String,
            private_key: String,
            client_email: String,
            token_uri: String,
        }
        let credentials: Credentials = serde_json::from_str(json).map_err(|_| invalid())?;
        if credentials.project_id.is_empty()
            || !credentials
                .project_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || credentials.client_email.is_empty()
        {
            return Err(invalid());
        }
        let pem = PrivatePkcs8KeyDer::from_pem_slice(credentials.private_key.as_bytes())
            .map_err(|_| invalid())?;
        let key = RsaKeyPair::from_pkcs8(pem.secret_pkcs8_der()).map_err(|_| invalid())?;
        let account = ServiceAccount {
            key,
            email: credentials.client_email,
            token_uri: credentials.token_uri,
        };
        account.assertion().map_err(|_| invalid())?;
        let endpoint = format!("{SEND_BASE}/{}/messages:send", credentials.project_id);
        let client = xmtp_common::http::client_builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(ATTEMPT_TIMEOUT)
            .build()
            .map_err(|_| invalid())?;
        Ok(Self {
            account: Arc::new(account),
            refresh: Mutex::new(None),
            client,
            endpoint,
        })
    }

    /// One cache owns the pending refresh and its result. Waiters share both
    /// success and failure. The inner deadline survives cancellation of a caller.
    async fn token(&self) -> Result<Arc<str>, ()> {
        let flight = {
            let mut refresh = self.refresh.lock();
            if refresh.as_ref().is_none_or(|flight| {
                flight
                    .peek()
                    .is_some_and(|cached| cached.refresh_at <= Instant::now())
            }) {
                let account = self.account.clone();
                let client = self.client.clone();
                *refresh = Some(
                    async move {
                        let result =
                            xmtp_common::time::timeout(ATTEMPT_TIMEOUT, account.exchange(&client))
                                .with_subscriber(tracing::Dispatch::none())
                                .await
                                .map_err(|_| ())
                                .and_then(|result| result);
                        match result {
                            Ok(token) => token,
                            Err(()) => CachedToken {
                                value: Err(()),
                                refresh_at: Instant::now() + RETRY_DELAY,
                            },
                        }
                    }
                    .boxed()
                    .shared(),
                );
            }
            refresh
                .as_ref()
                .expect("token flight was installed")
                .clone()
        };
        flight.await.value
    }

    async fn attempt(&self, delivery: &Delivery) -> Outcome {
        let Ok(token) = self.token().await else {
            return Outcome::Transient { retry_after: None };
        };
        let body = serde_json::json!({"message": {
            "token": delivery.config.delivery,
            "data": delivery.payload,
            "android": {"priority": "HIGH"},
            "apns": {
                "headers": {"apns-priority": "5", "apns-push-type": "background"},
                "payload": {"aps": {"content-available": 1}}
            }
        }});
        let Ok(response) = self
            .client
            .post(&self.endpoint)
            .bearer_auth(token.as_ref())
            .json(&body)
            .send()
            .await
        else {
            return Outcome::Transient { retry_after: None };
        };
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(retry_after);
        let Ok(body) = response_body(response).await else {
            return Outcome::Transient { retry_after: None };
        };
        classify(status, &body, retry_after)
    }
}

impl ServiceAccount {
    /// Use ring for RSA private-key operations. The rsa crate is used only by
    /// public-key verification and test fixtures because of its timing advisory.
    fn assertion(&self) -> Result<String, ()> {
        let iat = xmtp_common::time::now_secs();
        let claims = serde_json::to_vec(&Claims {
            iss: &self.email,
            scope: SCOPE,
            aud: &self.token_uri,
            iat,
            exp: iat + TOKEN_LIFETIME_SECONDS,
        })
        .map_err(|_| ())?;
        let mut assertion = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#),
            URL_SAFE_NO_PAD.encode(claims)
        );
        let mut signature = vec![0; self.key.public().modulus_len()];
        self.key
            .sign(
                &RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                assertion.as_bytes(),
                &mut signature,
            )
            .map_err(|_| ())?;
        assertion.push('.');
        URL_SAFE_NO_PAD.encode_string(signature, &mut assertion);
        Ok(assertion)
    }

    /// Exchange a signed assertion without redirects or ambient credentials.
    /// Token error bodies use the same byte cap as provider send responses.
    async fn exchange(&self, client: &reqwest::Client) -> Result<CachedToken, ()> {
        let assertion = self.assertion().map_err(|_| ())?;
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer")
            .append_pair("assertion", &assertion)
            .finish();
        let response = client
            .post(&self.token_uri)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|_| ())?;
        let status = response.status();
        let body = response_body(response).await?;
        if status != reqwest::StatusCode::OK {
            return Err(());
        }
        #[derive(Deserialize)]
        struct Token {
            access_token: String,
            expires_in: u64,
            token_type: String,
        }
        let token: Token = serde_json::from_slice(&body).map_err(|_| ())?;
        if token.access_token.is_empty()
            || !token.token_type.eq_ignore_ascii_case("Bearer")
            || token.expires_in <= TOKEN_REFRESH_MARGIN.as_secs()
        {
            return Err(());
        }
        Ok(CachedToken {
            value: Ok(token.access_token.into()),
            refresh_at: Instant::now()
                + Duration::from_secs(token.expires_in.min(TOKEN_LIFETIME_SECONDS as u64))
                - TOKEN_REFRESH_MARGIN,
        })
    }
}

async fn response_body(mut response: reqwest::Response) -> Result<Vec<u8>, ()> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[async_trait::async_trait]
impl Sender for FcmSender {
    async fn send(&self, delivery: &Delivery) -> Outcome {
        xmtp_common::time::timeout(ATTEMPT_TIMEOUT, self.attempt(delivery))
            .with_subscriber(tracing::Dispatch::none())
            .await
            .unwrap_or(Outcome::Transient { retry_after: None })
    }
}

fn invalid() -> ConfigError {
    ConfigError::Invalid {
        field: "push.fcm.service_account",
        reason: "must contain a valid service account, project_id, and Google token_uri",
    }
}

/// Only the typed FCM detail can identify a dead registration or sender mismatch.
fn classify(status: u16, body: &[u8], retry_after: Option<Duration>) -> Outcome {
    if status == 200 {
        return Outcome::Delivered;
    }
    #[derive(Deserialize)]
    struct Response {
        error: Failure,
    }
    #[derive(Deserialize)]
    struct Failure {
        #[serde(default)]
        details: Vec<serde_json::Value>,
    }
    #[derive(Deserialize)]
    struct Detail {
        #[serde(rename = "@type")]
        kind: String,
        #[serde(rename = "errorCode")]
        code: String,
    }
    let response = serde_json::from_slice::<Response>(body).ok();
    let code = response.and_then(|response| {
        response
            .error
            .details
            .into_iter()
            .filter_map(|value| serde_json::from_value::<Detail>(value).ok())
            .find(|detail| detail.kind == FCM_ERROR_TYPE)
            .map(|detail| detail.code)
    });
    match code.as_deref() {
        Some("UNREGISTERED") => Outcome::Terminal,
        Some("SENDER_ID_MISMATCH") => Outcome::Mismatch,
        Some("QUOTA_EXCEEDED") => Outcome::Transient {
            retry_after: Some(retry_after.unwrap_or(QUOTA_DELAY).max(QUOTA_DELAY)),
        },
        Some("UNAVAILABLE" | "INTERNAL") => Outcome::Transient { retry_after },
        _ => Outcome::Rejected,
    }
}

/// Retry-After accepts decimal seconds and HTTP dates. Never exceed five minutes.
fn retry_after(value: &str) -> Option<Duration> {
    let duration = if let Ok(seconds) = value.parse::<u64>() {
        Duration::from_secs(seconds)
    } else {
        httpdate::parse_http_date(value)
            .ok()?
            .duration_since(std::time::SystemTime::now())
            .unwrap_or_default()
    };
    Some(duration.min(MAX_RETRY_DELAY))
}

#[cfg(test)]
pub(crate) mod tests;
