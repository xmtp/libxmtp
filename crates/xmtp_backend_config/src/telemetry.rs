use super::{ConfigError, UrlKind, invalid, non_empty_url};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, net::SocketAddr};

const DEFAULT_METRICS_LISTEN: &str = "0.0.0.0:9464";
const DEFAULT_SERVICE_NAME: &str = "xmtp-backend";
const SAMPLE_ALL: f64 = 1.0;
const SAMPLE_NONE: f64 = 0.0;
const ENDPOINT_ENV: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
/// The deployment identity the backend attaches to every exported signal.
/// Reserved the same way `service.name` is: the backend owns its value.
pub const IDENTIFIER_ATTRIBUTE: &str = "xmtp.backend.identifier";

/// Metrics and optional OTLP export. Endpoint values are never included in errors.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TelemetryConfig {
    /// Separate Prometheus listener. An empty string disables the listener.
    #[schemars(schema_with = "super::schema::metrics_address")]
    pub metrics_listen: String,
    /// OTLP gRPC endpoint; absent uses OTEL_EXPORTER_OTLP_ENDPOINT.
    #[schemars(schema_with = "super::schema::optional_http_url")]
    pub otlp_endpoint: Option<String>,
    pub otlp_logs: bool,
    pub service_name: String,
    #[schemars(range(min = SAMPLE_NONE, max = SAMPLE_ALL))]
    pub sample_ratio: f64,
    #[schemars(schema_with = "super::schema::resource_attributes")]
    pub resource_attributes: BTreeMap<String, String>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            metrics_listen: DEFAULT_METRICS_LISTEN.into(),
            otlp_endpoint: None,
            otlp_logs: false,
            service_name: DEFAULT_SERVICE_NAME.into(),
            sample_ratio: SAMPLE_ALL,
            resource_attributes: BTreeMap::new(),
        }
    }
}

impl TelemetryConfig {
    pub(super) fn validate(&self) -> Result<(), ConfigError> {
        if !self.metrics_listen.is_empty() && self.metrics_listen.parse::<SocketAddr>().is_err() {
            return Err(invalid(
                "telemetry.metrics_listen",
                "must be a socket address or empty",
            ));
        }
        if !self.sample_ratio.is_finite()
            || !(SAMPLE_NONE..=SAMPLE_ALL).contains(&self.sample_ratio)
        {
            return Err(invalid(
                "telemetry.sample_ratio",
                "must be between zero and one",
            ));
        }
        if self.service_name.trim().is_empty() {
            return Err(invalid("telemetry.service_name", "must not be empty"));
        }
        for key in ["service.name", "service.version", IDENTIFIER_ATTRIBUTE] {
            if self.resource_attributes.contains_key(key) {
                return Err(invalid(
                    "telemetry.resource_attributes",
                    "service.name, service.version, and xmtp.backend.identifier are reserved",
                ));
            }
        }
        if let Some(endpoint) = &self.otlp_endpoint {
            validate_endpoint(endpoint, "telemetry.otlp_endpoint")?;
        }
        Ok(())
    }

    /// Resolve the one implicit environment fallback before any exporter is
    /// built: `otlp_endpoint`, else `OTEL_EXPORTER_OTLP_ENDPOINT`, else none.
    pub fn otlp_endpoint_from_env(&self) -> Result<Option<String>, ConfigError> {
        let fallback = match std::env::var(ENDPOINT_ENV) {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(_) if self.otlp_endpoint.is_some() => None,
            Err(_) => return Err(invalid(ENDPOINT_ENV, "must be a valid URL")),
        };
        self.otlp_endpoint(fallback)
    }

    /// The checked OTLP endpoint to export to: the configured one, else
    /// `env_fallback`, else none. Errors name the source, never the value.
    pub fn otlp_endpoint(
        &self,
        env_fallback: Option<String>,
    ) -> Result<Option<String>, ConfigError> {
        self.validate()?;
        let (endpoint, field) = match &self.otlp_endpoint {
            Some(endpoint) => (endpoint.clone(), "telemetry.otlp_endpoint"),
            None => match env_fallback {
                Some(endpoint) => (endpoint, ENDPOINT_ENV),
                None => return Ok(None),
            },
        };
        validate_endpoint(&endpoint, field)?;
        Ok(Some(endpoint))
    }
}

fn validate_endpoint(endpoint: &str, field: &'static str) -> Result<(), ConfigError> {
    non_empty_url(endpoint, field, UrlKind::Http)?;
    let uri = endpoint
        .parse::<http::Uri>()
        .map_err(|_| invalid(field, "must be a valid HTTP(S) URL"))?;
    if uri.host().is_none() {
        return Err(invalid(field, "must be a valid HTTP(S) URL"));
    }
    Ok(())
}
