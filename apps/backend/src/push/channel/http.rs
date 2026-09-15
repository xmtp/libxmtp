//! HTTPS webhook attempts with checked, pinned DNS results.

use super::{ATTEMPT_TIMEOUT, Delivery, Outcome, Sender};
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use std::net::SocketAddr;
use std::sync::Arc;

#[async_trait::async_trait]
trait Resolver: Send + Sync {
    async fn resolve(&self, host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>>;
}

struct SystemResolver;

#[async_trait::async_trait]
impl Resolver for SystemResolver {
    async fn resolve(&self, host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
        Ok(tokio::net::lookup_host((host, port)).await?.collect())
    }
}

pub(crate) struct HttpSender {
    allow_private: bool,
    allowed_domains: Option<Vec<String>>,
    resolver: Arc<dyn Resolver>,
    #[cfg(test)]
    trusted_root: Option<reqwest::Certificate>,
}

struct ValidatedDelivery<'a> {
    url: url::Url,
    client: reqwest::Client,
    key: &'a [u8],
    body: Vec<u8>,
}

impl HttpSender {
    pub fn new(config: &crate::config::push::HttpConfig) -> Self {
        Self {
            allow_private: config.allow_private_addresses,
            allowed_domains: config.allowed_domains.clone(),
            resolver: Arc::new(SystemResolver),
            #[cfg(test)]
            trusted_root: None,
        }
    }

    /// Validate one attempt and prepare its signing data and pinned client.
    /// DNS failures are transient; invalid delivery fields are rejected.
    async fn validate_delivery<'a>(
        &self,
        delivery: &'a Delivery,
    ) -> Result<ValidatedDelivery<'a>, Outcome> {
        let url = parse_delivery_url(&delivery.config.delivery)?;
        let host = url.host().ok_or(Outcome::Rejected)?;
        if let Some(domains) = &self.allowed_domains
            && !domains.is_empty()
            && !domains.iter().any(|domain| {
                crate::service::notification::webhook_url::matches_domain(&host.to_string(), domain)
            })
        {
            return Err(Outcome::Rejected);
        }
        let port = url.port_or_known_default().unwrap_or(443);
        let addresses: Vec<SocketAddr> = match host {
            url::Host::Ipv4(ip) => vec![SocketAddr::new(ip.into(), port)],
            url::Host::Ipv6(ip) => vec![SocketAddr::new(ip.into(), port)],
            url::Host::Domain(name) => self
                .resolver
                .resolve(name, port)
                .await
                .map_err(|_| Outcome::Transient { retry_after: None })?,
        };
        if addresses.is_empty() {
            return Err(Outcome::Transient { retry_after: None });
        }
        if !self.allow_private
            && addresses
                .iter()
                .any(|address| crate::service::notification::webhook_url::blocked(address.ip()))
        {
            return Err(Outcome::Rejected);
        }
        let builder = xmtp_common::http::client_builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(ATTEMPT_TIMEOUT);
        let builder = match host {
            url::Host::Domain(name) => builder.resolve_to_addrs(name, &addresses),
            _ => builder,
        };
        #[cfg(test)]
        let builder = if let Some(root) = &self.trusted_root {
            builder.add_root_certificate(root.clone())
        } else {
            builder
        };
        let client = builder.build().map_err(|_| Outcome::Rejected)?;
        let key = delivery
            .config
            .signing_key
            .as_deref()
            .ok_or(Outcome::Rejected)?;
        let body = body(delivery).map_err(|_| Outcome::Rejected)?;
        Ok(ValidatedDelivery {
            url,
            client,
            key,
            body,
        })
    }

    /// The outer deadline includes DNS, connection establishment, and response
    /// headers. Proxy and redirect handling cannot bypass the checked address.
    async fn attempt(&self, delivery: &Delivery) -> Outcome {
        let ValidatedDelivery {
            url,
            client,
            key,
            body,
        } = match self.validate_delivery(delivery).await {
            Ok(validated) => validated,
            Err(outcome) => return outcome,
        };
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = xmtp_common::time::now_secs().to_string();
        let signature = signature(key, &id, &timestamp, &body);
        match client
            .post(url)
            .header("content-type", "application/json")
            .header("webhook-id", id)
            .header("webhook-timestamp", timestamp)
            .header("webhook-signature", signature)
            .body(body)
            .send()
            .await
        {
            Ok(response) => classify(response.status().as_u16()),
            Err(_) => Outcome::Transient { retry_after: None },
        }
    }
}

/// Parse an HTTPS delivery URL without accepting credentials in the URL.
fn parse_delivery_url(value: &str) -> Result<url::Url, Outcome> {
    let url = url::Url::parse(value).map_err(|_| Outcome::Rejected)?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err(Outcome::Rejected);
    }
    Ok(url)
}

#[async_trait::async_trait]
impl Sender for HttpSender {
    async fn send(&self, delivery: &Delivery) -> Outcome {
        xmtp_common::time::timeout(ATTEMPT_TIMEOUT, self.attempt(delivery))
            .await
            .unwrap_or(Outcome::Transient { retry_after: None })
    }
}

fn classify(status: u16) -> Outcome {
    match status {
        200..=299 => Outcome::Delivered,
        404 | 410 => Outcome::GoneTransient,
        500..=599 => Outcome::Transient { retry_after: None },
        _ => Outcome::Rejected,
    }
}

fn body(delivery: &Delivery) -> Result<Vec<u8>, serde_json::Error> {
    #[derive(Serialize)]
    struct Body<'a> {
        #[serde(flatten)]
        payload: &'a xmtp_push_types::PushPayload,
        recipient_id: String,
    }
    serde_json::to_vec(&Body {
        payload: &delivery.payload,
        recipient_id: hex::encode(&delivery.config.recipient_id),
    })
}

fn signature(key: &[u8], id: &str, timestamp: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(id.as_bytes());
    mac.update(b".");
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
pub(crate) mod tests;
