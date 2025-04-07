pub mod grpc_api_helper;
pub mod grpc_client;
mod identity;

pub const LOCALHOST_ADDRESS: &str = "http://localhost:5556";
pub const DEV_ADDRESS: &str = "https://grpc.dev.xmtp.network:443";

pub use grpc_api_helper::{Client, GroupMessageStream, WelcomeMessageStream};
use thiserror::Error;
use xmtp_proto::ConversionError;

#[derive(Debug, Error)]
pub enum GrpcBuilderError {
    #[error("app version required to create client")]
    MissingAppVersion,
    #[error("libxmtp core library version required to create client")]
    MissingLibxmtpVersion,
    #[error("host url required to create client")]
    MissingHostUrl,
    #[error("payer url required to create client")]
    MissingPayerUrl,
    #[error(transparent)]
    Metadata(#[from] tonic::metadata::errors::InvalidMetadataValue),
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),
    #[error("Invalid URI during channel creation")]
    InvalidUri(#[from] http::uri::InvalidUri),
}

#[derive(Debug, Error)]
pub enum GrpcError {
    #[error("Invalid URI during channel creation")]
    InvalidUri(#[from] http::uri::InvalidUri),
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),
    #[error(transparent)]
    Metadata(#[from] tonic::metadata::errors::InvalidMetadataValue),
    #[error(transparent)]
    Status(#[from] tonic::Status),
    #[error("{0} not found/empty")]
    NotFound(String),
    #[error("Payload not expected")]
    UnexpectedPayload,
    #[error("payload is missing")]
    MissingPayload,
    #[error(transparent)]
    Proto(#[from] xmtp_proto::ProtoError),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error("unreachable (Infallible)")]
    Unreachable,
}

impl From<ConversionError> for GrpcError {
    fn from(error: ConversionError) -> Self {
        GrpcError::NotFound(error.to_string())
    }
}

impl xmtp_common::retry::RetryableError for GrpcError {
    fn is_retryable(&self) -> bool {
        true
    }
}

impl xmtp_proto::XmtpApiError for GrpcError {
    fn api_call(&self) -> Option<xmtp_proto::ApiEndpoint> {
        None
    }

    fn code(&self) -> Option<xmtp_proto::Code> {
        match &self {
            GrpcError::Status(status) => Some(status.code().into()),
            _ => None,
        }
    }

    fn grpc_message(&self) -> Option<&str> {
        match &self {
            GrpcError::Status(status) => Some(status.message()),
            _ => None,
        }
    }
}

#[cfg(test)]
pub mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use xmtp_proto::api_client::ApiBuilder;
    use xmtp_proto::xmtp::message_api::v1::{Envelope, PublishRequest};

    // Return the json serialization of an Envelope with bytes
    pub fn test_envelope(topic: String) -> Envelope {
        let time_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        Envelope {
            timestamp_ns: time_since_epoch.as_nanos() as u64,
            content_topic: topic,
            message: vec![65],
        }
    }

    #[tokio::test]
    async fn metadata_test() {
        let mut client = Client::builder();
        client.set_host(DEV_ADDRESS.to_string());
        client.set_tls(true);
        let app_version = "test/1.0.0".to_string();
        let libxmtp_version = "0.0.1".to_string();
        client.set_app_version(app_version.clone()).unwrap();
        client.set_libxmtp_version(libxmtp_version.clone()).unwrap();
        let client = client.build().await.unwrap();
        let request = client.build_request(PublishRequest { envelopes: vec![] });

        assert_eq!(
            request
                .metadata()
                .get("x-app-version")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
            app_version
        );
        assert_eq!(
            request
                .metadata()
                .get("x-libxmtp-version")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
            libxmtp_version
        );
    }
}
