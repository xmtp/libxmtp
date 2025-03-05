use crate::verified_key_package_v2::KeyPackageVerificationError;
use openmls::framing::errors::ProtocolMessageError;
use xmtp_common::RetryableError;
use xmtp_proto::{ApiEndpoint, Code, XmtpApiError};

#[derive(thiserror::Error, Debug)]
pub enum LocalClientError {
    #[error(transparent)]
    KeyPackageVerification(#[from] KeyPackageVerificationError),
    #[error(transparent)]
    TlsCodec(#[from] tls_codec::Error),
    #[error(transparent)]
    Protocol(#[from] ProtocolMessageError),
}

impl XmtpApiError for LocalClientError {
    fn api_call(&self) -> Option<ApiEndpoint> {
        None
    }
    fn code(&self) -> Option<Code> {
        None
    }
    fn grpc_message(&self) -> Option<&str> {
        None
    }
}

impl RetryableError for LocalClientError {
    fn is_retryable(&self) -> bool {
        false
    }
}
