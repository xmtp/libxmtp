//! Bounded JWKS fetches and monotonic refresh deadlines.
use super::keys::{KeySet, VerifyingKey, from_jwk};
use crate::config::auth::{
    AuthConfig, JITTER_DIVISOR, JWKS_FETCH_TIMEOUT, is_loopback, parse_jwks_url,
};
use futures::StreamExt;
use reqwest::redirect::Policy;
use xmtp_common::{
    Retry, Strategy,
    time::{Duration, Instant, sleep},
};

pub(crate) const MAX_JWKS_BYTES: usize = 256 * 1024;
pub(crate) const MAX_JWKS_KEYS: usize = 64;
/// Entries examined per document. Each unusable entry costs one warning, so a
/// hostile or broken endpoint must not be able to drive that count with a
/// large document on a short refresh period.
pub(crate) const MAX_JWKS_ENTRIES: usize = 256;
pub(crate) const JWKS_STARTUP_ATTEMPTS: usize = 3;
pub(crate) const JWKS_STARTUP_RETRY_DELAY: Duration = Duration::from_secs(1);

#[derive(Debug, thiserror::Error)]
#[error("JWKS fetch failed for host {host}")]
pub(crate) struct FetchError {
    host: String,
}
impl xmtp_common::RetryableError for FetchError {
    fn is_retryable(&self) -> bool {
        true
    }
}
struct FixedRetry;
impl Strategy for FixedRetry {
    fn backoff(&self, _: usize, _: Instant) -> Option<Duration> {
        Some(JWKS_STARTUP_RETRY_DELAY)
    }
}

#[derive(Clone)]
pub(crate) struct JwksSource {
    client: reqwest::Client,
    url: url::Url,
    config: AuthConfig,
}
impl JwksSource {
    /// Disable redirects and bound each fetch, including the response body.
    pub fn new(
        url: &str,
        config: AuthConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let url = parse_jwks_url(url)?;
        let client = xmtp_common::http::client_builder()
            .redirect(Policy::none())
            .https_only(!is_loopback(&url))
            .timeout(JWKS_FETCH_TIMEOUT)
            .build()
            .map_err(|_| FetchError {
                host: url.host_str().unwrap_or_default().to_owned(),
            })?;
        Ok(Self {
            client,
            url,
            config,
        })
    }
    pub fn host(&self) -> &str {
        self.url.host_str().unwrap_or_default()
    }
    fn error(&self) -> FetchError {
        FetchError {
            host: self.host().to_owned(),
        }
    }

    /// Read at most the body cap. Never buffer an unbounded response.
    /// Every failure is reduced to a host-only error before retry or logging.
    pub async fn fetch(&self) -> Result<Vec<VerifyingKey>, FetchError> {
        let response = self
            .client
            .get(self.url.clone())
            .send()
            .await
            .map_err(|_| self.error())?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|size| size > MAX_JWKS_BYTES as u64)
        {
            return Err(self.error());
        }
        let mut stream = response.bytes_stream();
        let mut body = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| self.error())?;
            if chunk.len() > MAX_JWKS_BYTES - body.len() {
                return Err(self.error());
            }
            body.extend_from_slice(&chunk);
        }
        let document: serde_json::Value =
            serde_json::from_slice(&body).map_err(|_| self.error())?;
        let entries = document
            .get("keys")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| self.error())?;
        // Bound the entries examined, not just the keys kept. `from_jwk` warns
        // once per unusable entry, so filtering the whole document first lets a
        // malformed 256 KiB response emit thousands of lines on every refresh.
        let examined = entries.len().min(MAX_JWKS_ENTRIES);
        let mut usable = entries[..examined]
            .iter()
            .filter_map(|value| from_jwk(value, &self.config));
        let keys: Vec<_> = usable.by_ref().take(MAX_JWKS_KEYS).collect();
        if usable.next().is_some() || entries.len() > examined {
            tracing::warn!(
                examined,
                total = entries.len(),
                "JWKS limit reached; keeping the first usable keys"
            );
        }
        if keys.is_empty() {
            return Err(self.error());
        }
        Ok(keys)
    }

    /// Try three times with one second between failures before startup can continue.
    pub async fn startup(&self) -> Result<Vec<VerifyingKey>, FetchError> {
        let retry = Retry::builder()
            .retries(JWKS_STARTUP_ATTEMPTS - 1)
            .with_strategy(FixedRetry)
            .build();
        xmtp_common::retry_async!(retry, (async { self.fetch().await }))
    }

    /// Keep the last successful snapshot until its monotonic deadline.
    /// Returning signals the caller to use the normal bounded shutdown drain.
    /// Canceling this future also cancels any fetch in progress.
    pub async fn refresh(&self, keys: &KeySet, mut last_success: Instant) {
        let period = Duration::from_secs(self.config.jwks_refresh_seconds);
        let max_stale = Duration::from_secs(self.config.jwks_max_stale_seconds);
        // Validation checks representability against an earlier instant, so the
        // sum can still overflow here. Ending the loop drains rather than panics.
        while let Some(deadline) = last_success.checked_add(max_stale) {
            let tick =
                sleep(period + xmtp_common::time::rand_offset(period / JITTER_DIVISOR as u32));
            tokio::select! {
                biased;
                _ = sleep_until(deadline) => break,
                _ = tick => {}
            }
            let result = tokio::select! {
                biased;
                _ = sleep_until(deadline) => { crate::telemetry::auth_jwks_refresh(false); break; },
                result = self.fetch() => result,
            };
            if Instant::now() >= deadline {
                // The result is discarded: a late fetch must not rescue the
                // server. Count what the fetch did, not what the deadline did.
                crate::telemetry::auth_jwks_refresh(result.is_ok());
                break;
            }
            crate::telemetry::auth_jwks_refresh(result.is_ok());
            match result {
                Ok(loaded) => {
                    keys.replace(loaded);
                    last_success = Instant::now();
                }
                Err(_) => tracing::warn!(host = self.host(), "JWKS refresh failed"),
            }
        }
        tracing::error!(
            host = self.host(),
            "JWKS keys exceeded the maximum stale time"
        );
    }
}

async fn sleep_until(deadline: Instant) {
    sleep(deadline.saturating_duration_since(Instant::now())).await;
}

#[cfg(test)]
mod tests;
