use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use aws_config::{BehaviorVersion, Region, provider_config::ProviderConfig};
use aws_credential_types::{
    Credentials,
    provider::{ProvideCredentials, SharedCredentialsProvider, error::CredentialsError},
};
use aws_sigv4::{
    http_request::{
        PercentEncodingMode, SignableBody, SignableRequest, SignatureLocation, SigningSettings,
        UriPathNormalizationMode, sign,
    },
    sign::v4,
};
use aws_smithy_http_client::{
    Builder as HttpClientBuilder,
    tls::{Provider as TlsProvider, rustls_provider::CryptoMode},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use tokio::sync::Mutex;
use url::Url;

use crate::config::{MAX_PRESIGN_TTL_SECONDS, MIN_PRESIGN_TTL_SECONDS};
use crate::{AttachmentsConfig, ConfigInvalid, CredentialsConfig, S3Config, TargetConfig};

/// The request that an attachment client sends to the storage target.
#[derive(Clone)]
pub struct PresignedRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub expires_in_seconds: u32,
}

impl std::fmt::Debug for PresignedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PresignedRequest")
            .field("method", &self.method)
            .field("url", &"[redacted]")
            .field("headers", &"[redacted]")
            .field("expires_in_seconds", &self.expires_in_seconds)
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SignError {
    #[error("storage credentials unavailable")]
    CredentialsUnavailable,
    #[error("storage request could not be signed")]
    SigningFailed,
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error(transparent)]
    Config(#[from] ConfigInvalid),
    #[error("storage credentials provider could not be configured")]
    CredentialsProvider,
}

#[async_trait]
pub trait StorageTarget: Send + Sync {
    async fn presign_put(
        &self,
        content_digest: &[u8; 32],
        content_length: u64,
    ) -> Result<PresignedRequest, SignError>;
}

/// Create a target after validating all operator settings.
pub async fn build_target(
    config: &AttachmentsConfig,
) -> Result<Arc<dyn StorageTarget>, BuildError> {
    config.validate()?;
    match &config.target {
        TargetConfig::S3(s3) => Ok(Arc::new(S3Target::new(s3).await?)),
    }
}

/// S3-compatible presigner. It does not contact the target.
pub struct S3Target {
    endpoint: Url,
    region: String,
    bucket: String,
    key_prefix: String,
    ttl: u32,
    provider: SharedCredentialsProvider,
    cached: Mutex<Option<Credentials>>,
    clock: Arc<dyn Fn() -> SystemTime + Send + Sync>,
}

impl std::fmt::Debug for S3Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Target")
            .field("endpoint", &"[redacted]")
            .field("region", &self.region)
            .field("bucket", &"[redacted]")
            .field("credentials", &"[redacted]")
            .finish()
    }
}

impl S3Target {
    pub async fn new(config: &S3Config) -> Result<Self, BuildError> {
        Self::build_with_clock(config, Arc::new(SystemTime::now)).await
    }

    /// Create a target with a supplied clock for deterministic signing.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_with_clock(
        config: &S3Config,
        clock: Arc<dyn Fn() -> SystemTime + Send + Sync>,
    ) -> Result<Self, BuildError> {
        Self::build_with_clock(config, clock).await
    }

    async fn build_with_clock(
        config: &S3Config,
        clock: Arc<dyn Fn() -> SystemTime + Send + Sync>,
    ) -> Result<Self, BuildError> {
        config.validate()?;
        let provider = provider_for(config).await?;
        Ok(Self::with_provider(config, provider, clock))
    }

    fn with_provider(
        config: &S3Config,
        provider: SharedCredentialsProvider,
        clock: Arc<dyn Fn() -> SystemTime + Send + Sync>,
    ) -> Self {
        // The validated endpoint is a URL. Recheck at this private boundary for test construction.
        let endpoint = Url::parse(&config.endpoint).expect("validated S3 endpoint");
        Self {
            endpoint,
            region: config.region.clone(),
            bucket: config.bucket.clone(),
            key_prefix: config.key_prefix.clone(),
            ttl: config.presign_ttl_seconds(),
            provider,
            cached: Mutex::new(None),
            clock,
        }
    }

    /// Keep a credential until less than five minutes remain, then ask the provider again.
    /// The lock also makes concurrent refreshes one operation.
    async fn credentials(&self) -> Result<(Credentials, SystemTime), SignError> {
        let mut cached = self.cached.lock().await;
        let now = (self.clock)();
        if let Some(credentials) = cached.as_ref()
            && credential_ttl(credentials, now, self.ttl).is_some()
        {
            return Ok((credentials.clone(), now));
        }
        let credentials = self.provider.provide_credentials().await.map_err(|error| {
            let kind = match error {
                CredentialsError::CredentialsNotLoaded(_) => "not_loaded",
                CredentialsError::ProviderTimedOut(_) => "timed_out",
                CredentialsError::InvalidConfiguration(_) => "invalid_configuration",
                CredentialsError::ProviderError(_) => "provider_error",
                CredentialsError::Unhandled(_) => "unhandled",
                _ => "unknown",
            };
            tracing::warn!(
                credential_error_kind = kind,
                "attachment credentials unavailable"
            );
            SignError::CredentialsUnavailable
        })?;
        let now = (self.clock)();
        credential_ttl(&credentials, now, self.ttl).ok_or(SignError::CredentialsUnavailable)?;
        *cached = Some(credentials.clone());
        Ok((credentials, now))
    }

    fn object_url(&self, digest: &[u8; 32]) -> Result<Url, SignError> {
        let mut url = self.endpoint.clone();
        let key = format!("{}{}", self.key_prefix, hex::encode(digest));
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| SignError::SigningFailed)?;
        segments.pop_if_empty();
        segments.push(&self.bucket);
        for segment in key.split('/') {
            if !segment.is_empty() {
                segments.push(segment);
            }
        }
        drop(segments);
        Ok(url)
    }
}

fn credential_ttl(credentials: &Credentials, now: SystemTime, configured_ttl: u32) -> Option<u32> {
    let remaining = match credentials.expiry() {
        Some(expiry) => expiry
            .duration_since(now)
            .ok()?
            .as_secs()
            .min(u64::from(MAX_PRESIGN_TTL_SECONDS)) as u32,
        None => MAX_PRESIGN_TTL_SECONDS,
    };
    let ttl = configured_ttl.min(MAX_PRESIGN_TTL_SECONDS).min(remaining);
    (ttl >= MIN_PRESIGN_TTL_SECONDS).then_some(ttl)
}

#[async_trait]
impl StorageTarget for S3Target {
    async fn presign_put(
        &self,
        content_digest: &[u8; 32],
        content_length: u64,
    ) -> Result<PresignedRequest, SignError> {
        let (credentials, now) = self.credentials().await?;
        let ttl =
            credential_ttl(&credentials, now, self.ttl).ok_or(SignError::CredentialsUnavailable)?;
        let url = self.object_url(content_digest)?;
        let headers = vec![
            ("content-length".to_string(), content_length.to_string()),
            ("if-none-match".to_string(), "*".to_string()),
            (
                "x-amz-checksum-sha256".to_string(),
                STANDARD.encode(content_digest),
            ),
        ];
        let signed_url = sign_request("PUT", &url, &headers, &credentials, &self.region, now, ttl)?;
        Ok(PresignedRequest {
            method: "PUT".into(),
            url: signed_url,
            headers,
            expires_in_seconds: ttl,
        })
    }
}

/// Shared by the target and the AWS reference-vector test.
fn sign_request(
    method: &str,
    url: &Url,
    headers: &[(String, String)],
    credentials: &Credentials,
    region: &str,
    now: SystemTime,
    ttl: u32,
) -> Result<String, SignError> {
    let mut settings = SigningSettings::default();
    settings.percent_encoding_mode = PercentEncodingMode::Single;
    settings.uri_path_normalization_mode = UriPathNormalizationMode::Disabled;
    settings.signature_location = SignatureLocation::QueryParams;
    settings.expires_in = Some(Duration::from_secs(u64::from(ttl)));
    let identity = credentials.clone().into();
    let params = v4::SigningParams::builder()
        .identity(&identity)
        .region(region)
        .name("s3")
        .time(now)
        .settings(settings)
        .build()
        .map_err(|_| SignError::SigningFailed)?;
    let signable = SignableRequest::new(
        method,
        url.as_str(),
        headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
        SignableBody::UnsignedPayload,
    )
    .map_err(|_| SignError::SigningFailed)?;
    let (instructions, _) = sign(signable, &params.into())
        .map_err(|_| SignError::SigningFailed)?
        .into_parts();
    let mut request = http::Request::builder()
        .method(method)
        .uri(url.as_str())
        .body(())
        .map_err(|_| SignError::SigningFailed)?;
    instructions.apply_to_request_http1x(&mut request);
    Ok(request.uri().to_string())
}

async fn provider_for(config: &S3Config) -> Result<SharedCredentialsProvider, BuildError> {
    let provider_config = ProviderConfig::without_region()
        .with_region(Some(Region::new(config.region.clone())))
        .with_http_client(
            HttpClientBuilder::new()
                .tls_provider(TlsProvider::Rustls(CryptoMode::Ring))
                .build_https(),
        );
    let provider = match &config.credentials {
        CredentialsConfig::Static {
            access_key_id,
            secret_access_key,
            session_token,
        } => SharedCredentialsProvider::new(Credentials::new(
            access_key_id,
            secret_access_key,
            session_token.clone(),
            None,
            "attachment-static",
        )),
        CredentialsConfig::Environment => SharedCredentialsProvider::new(
            aws_config::environment::credentials::EnvironmentVariableCredentialsProvider::new(),
        ),
        CredentialsConfig::Profile { name } => SharedCredentialsProvider::new(
            aws_config::profile::ProfileFileCredentialsProvider::builder()
                .configure(&provider_config)
                .profile_name(name)
                .build(),
        ),
        CredentialsConfig::Sso {
            account_id,
            region,
            role_name,
            start_url,
            session_name,
        } => {
            let mut builder = aws_config::sso::credentials::SsoCredentialsProvider::builder()
                .configure(&provider_config)
                .account_id(account_id)
                .region(Region::new(region.clone()))
                .role_name(role_name)
                .start_url(start_url);
            if let Some(name) = session_name {
                builder = builder.session_name(name);
            }
            SharedCredentialsProvider::new(builder.build())
        }
        CredentialsConfig::Process { command } => SharedCredentialsProvider::new(
            aws_config::credential_process::CredentialProcessProvider::new(command.clone()),
        ),
        CredentialsConfig::WebIdentity => SharedCredentialsProvider::new(
            aws_config::web_identity_token::WebIdentityTokenCredentialsProvider::builder()
                .configure(&provider_config)
                .build(),
        ),
        CredentialsConfig::Container => SharedCredentialsProvider::new(
            aws_config::ecs::EcsCredentialsProvider::builder()
                .configure(&provider_config)
                .build(),
        ),
        CredentialsConfig::Instance => SharedCredentialsProvider::new(
            aws_config::imds::credentials::ImdsCredentialsProvider::builder()
                .configure(&provider_config)
                .build(),
        ),
        CredentialsConfig::DefaultChain | CredentialsConfig::AssumeRole { .. } => {
            let sdk = aws_config::defaults(BehaviorVersion::latest())
                .region(Region::new(config.region.clone()))
                .http_client(
                    HttpClientBuilder::new()
                        .tls_provider(TlsProvider::Rustls(CryptoMode::Ring))
                        .build_https(),
                )
                .load()
                .await;
            if let CredentialsConfig::AssumeRole {
                role_arn,
                external_id,
                session_name,
            } = &config.credentials
            {
                let mut builder =
                    aws_config::sts::AssumeRoleProvider::builder(role_arn).configure(&sdk);
                if let Some(id) = external_id {
                    builder = builder.external_id(id);
                }
                if let Some(name) = session_name {
                    builder = builder.session_name(name);
                }
                SharedCredentialsProvider::new(builder.build().await)
            } else {
                sdk.credentials_provider()
                    .ok_or(BuildError::CredentialsProvider)?
            }
        }
    };
    Ok(provider)
}

#[cfg(test)]
mod tests;
