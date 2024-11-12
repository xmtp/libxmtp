use std::fmt;
use std::string::FromUtf8Error;
use serde::de::StdError;
use openmls::prelude::tls_codec::Error as TlsCodecError;

#[derive(Debug)]
pub enum ErrorKind {
    SetupCreateChannelError,
    SetupTLSConfigError,
    SetupConnectionError,
    PublishError,
    QueryError,
    SubscribeError,
    BatchQueryError,
    MlsError,
    IdentityError,
    SubscriptionUpdateError,
    MetadataError,
    InternalError(InternalError)
}

#[derive(Debug)]
pub enum InternalError {
    MissingPayloadError,
    InvalidTopicError(String),
    DecodingError(String),
    TLSError(String),
}

type ErrorSource = Box<dyn StdError + Send + Sync + 'static>;

pub struct Error {
    kind: ErrorKind,
    source: Option<ErrorSource>,
}

impl Error {
    pub fn new(kind: ErrorKind) -> Self {
        Self { kind, source: None }
    }

    pub fn with(mut self, source: impl Into<ErrorSource>) -> Self {
        self.source = Some(source.into());
        self
    }
}

impl From<hex::FromHexError> for Error {
    fn from(err: hex::FromHexError) -> Self {
        Error::new(ErrorKind::InternalError(InternalError::DecodingError(err.to_string())))
    }
}

impl From<prost::DecodeError> for Error {
    fn from(err: prost::DecodeError) -> Self {
        Error::new(ErrorKind::InternalError(InternalError::DecodingError(err.to_string())))
    }
}

impl From<FromUtf8Error> for Error {
    fn from(err: FromUtf8Error) -> Self {
        Error::new(ErrorKind::InternalError(InternalError::DecodingError(err.to_string())))
    }
}

impl From<TlsCodecError> for Error {
    fn from(err: TlsCodecError) -> Self {
        Error::new(ErrorKind::InternalError(InternalError::TLSError(err.to_string())))
    }
}

impl From<InternalError> for Error {
    fn from(internal: InternalError) -> Self {
        Error::new(ErrorKind::InternalError(internal))
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_tuple("xmtp::error::Error");

        f.field(&self.kind);

        if let Some(source) = &self.source {
            f.field(source);
        }

        f.finish()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match &self.kind {
            ErrorKind::SetupCreateChannelError => "failed to create channel",
            ErrorKind::SetupTLSConfigError => "tls configuration failed",
            ErrorKind::SetupConnectionError => "connection failed",
            ErrorKind::PublishError => "publish error",
            ErrorKind::QueryError => "query error",
            ErrorKind::SubscribeError => "subscribe error",
            ErrorKind::BatchQueryError => "batch query error",
            ErrorKind::IdentityError => "identity error",
            ErrorKind::MlsError => "mls error",
            ErrorKind::SubscriptionUpdateError => "subscription update error",
            ErrorKind::MetadataError => "metadata error",
            ErrorKind::InternalError(internal) => match internal {
                InternalError::MissingPayloadError => "missing payload error",
                InternalError::InvalidTopicError(topic) => &format!("invalid topic error: {}", topic),
                InternalError::DecodingError(msg) => msg,
                InternalError::TLSError(msg) => msg,
            },
        };
        f.write_str(s)?;
        if self.source().is_some() {
            f.write_str(": ")?;
            f.write_str(&self.source().unwrap().to_string())?;
        }
        Ok(())
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_ref()
            .map(|source| &**source as &(dyn StdError + 'static))
    }
}