use serde::{Deserialize, Serialize};
use xmtp_proto::{ApiEndpoint, Code, XmtpApiError};

#[derive(thiserror::Error, Debug)]
pub struct Error {
    endpoint: Option<ApiEndpoint>,
    #[source]
    source: HttpClientError,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ErrorResponse {
    pub code: usize,
    pub message: String,
    pub details: Vec<String>,
}

impl From<ErrorResponse> for HttpClientError {
    fn from(e: ErrorResponse) -> HttpClientError {
        HttpClientError::Grpc(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(endpoint) = self.endpoint {
            write!(f, "endpoint {} errored with {}", endpoint, self.source)
        } else {
            write!(f, "{}", self.source)
        }
    }
}

impl xmtp_common::RetryableError for Error {
    fn is_retryable(&self) -> bool {
        self.source.is_retryable()
    }

    fn needs_cooldown(&self) -> bool {
        self.source.needs_cooldown()
    }
}

impl Error {
    pub fn new<I: Into<HttpClientError>>(source: I) -> Self {
        Self {
            source: source.into(),
            endpoint: None,
        }
    }

    pub fn with(mut self, endpoint: ApiEndpoint) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    pub fn subscribe_group_messages<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::SubscribeGroupMessages),
        }
    }

    pub fn subscribe_welcomes<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::SubscribeWelcomes),
        }
    }

    pub fn upload_kp<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::UploadKeyPackage),
        }
    }

    pub fn fetch_kps<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::FetchKeyPackages),
        }
    }

    pub fn send_group_messages<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::SendGroupMessages),
        }
    }

    pub fn send_welcome_messages<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::SendWelcomeMessages),
        }
    }

    pub fn query_group_messages<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::QueryGroupMessages),
        }
    }

    pub fn query_welcome_messages<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::QueryWelcomeMessages),
        }
    }

    pub fn publish_identity_update<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::PublishIdentityUpdate),
        }
    }

    pub fn get_inbox_ids<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::GetInboxIds),
        }
    }

    pub fn get_identity_updates_v2<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::GetIdentityUpdatesV2),
        }
    }

    pub fn verify_scw_signature<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::VerifyScwSignature),
        }
    }

    pub fn query_v4_envelopes<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::QueryV4Envelopes),
        }
    }

    pub fn publish_envelopes<E: Into<HttpClientError>>(e: E) -> Self {
        Self {
            source: e.into(),
            endpoint: Some(ApiEndpoint::PublishEnvelopes),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HttpClientError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("grpc error {} at http gateway {}", _0.code, _0.message)]
    Grpc(ErrorResponse),
    #[error(transparent)]
    HeaderValue(#[from] reqwest::header::InvalidHeaderValue),
    #[error(transparent)]
    HeaderName(#[from] reqwest::header::InvalidHeaderName),
    #[error("error deserializing json response {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
}

impl xmtp_common::RetryableError for HttpClientError {
    fn is_retryable(&self) -> bool {
        true
    }

    fn needs_cooldown(&self) -> bool {
        match self {
            Self::Grpc(e) => (Code::from(e.code)) == Code::ResourceExhausted,
            _ => false,
        }
    }
}

impl From<HttpClientError> for Error {
    fn from(v: HttpClientError) -> Error {
        Error {
            endpoint: None,
            source: v,
        }
    }
}

impl XmtpApiError for HttpClientError {
    fn api_call(&self) -> Option<ApiEndpoint> {
        None
    }

    fn code(&self) -> Option<Code> {
        match self {
            Self::Grpc(e) => Some(e.code.into()),
            _ => None,
        }
    }

    fn grpc_message(&self) -> Option<&str> {
        match self {
            Self::Grpc(e) => Some(&e.message),
            _ => None,
        }
    }
}

impl XmtpApiError for Error {
    fn api_call(&self) -> Option<ApiEndpoint> {
        self.endpoint
    }

    fn code(&self) -> Option<Code> {
        match &self.source {
            HttpClientError::Grpc(e) => Some(e.code.into()),
            _ => None,
        }
    }

    fn grpc_message(&self) -> Option<&str> {
        match &self.source {
            HttpClientError::Grpc(e) => Some(e.message.as_str()),
            _ => None,
        }
    }
}
