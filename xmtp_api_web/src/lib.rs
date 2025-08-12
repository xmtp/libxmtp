#![warn(clippy::unwrap_used)]

use thiserror::Error;
use tonic_web_wasm_client::Client as WasmClient;

pub mod web_client;
mod error;
pub use error::*;


#[cfg(test)]
mod test;

#[macro_use]
extern crate tracing;

use tonic::metadata::{self, MetadataValue};
use xmtp_configuration::GRPC_PAYLOAD_LIMIT;
use xmtp_proto::prelude::ApiBuilder;

use crate::web_client::GrpcWebClient;

#[derive(Error, Debug)]
pub enum ClientBuilderError {
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
    #[error("Invalid URI during channel creation")]
    InvalidUri(#[from] http::uri::InvalidUri),
    #[error(transparent)]
    Url(#[from] url::ParseError),
}

// much of this can be extracted/combined with the normal GRPC builder
#[derive(Debug, Clone)]
pub struct ClientBuilder {
    host: Option<String>,
    app_version: Option<MetadataValue<metadata::Ascii>>,
    libxmtp_version: Option<MetadataValue<metadata::Ascii>>,
    // rate per minute
    limit: Option<u64>,
}

impl ApiBuilder for ClientBuilder {
    type Output = GrpcWebClient;
    type Error = ClientBuilderError;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        self.libxmtp_version = Some(MetadataValue::try_from(&version)?);
        Ok(())
    }

    fn set_app_version(&mut self, version: String) -> Result<(), Self::Error> {
        self.app_version = Some(MetadataValue::try_from(&version)?);
        Ok(())
    }

    fn set_host(&mut self, host: String) {
        self.host = Some(host);
    }

    fn rate_per_minute(&mut self, limit: u32) {
        self.limit = Some(limit.into());
    }

    async fn build(self) -> Result<Self::Output, Self::Error> {
        let host = self.host.ok_or(ClientBuilderError::MissingHostUrl)?;
        tracing::info!("building GrpcClient with host {}", &host);
        let client = WasmClient::new(host);
        Ok(GrpcWebClient {
            inner: tonic::client::Grpc::new(client)
                .max_decoding_message_size(GRPC_PAYLOAD_LIMIT)
                .max_encoding_message_size(GRPC_PAYLOAD_LIMIT),
            app_version: self
                .app_version
                .unwrap_or(MetadataValue::try_from("0.0.0")?),
            libxmtp_version: self.libxmtp_version.unwrap_or(MetadataValue::try_from(
                env!("CARGO_PKG_VERSION").to_string(),
            )?),
        })
    }

    fn set_tls(&mut self, _tls: bool) {}

    fn port(&self) -> Result<Option<String>, Self::Error> {
        if let Some(h) = &self.host {
            let u = url::Url::parse(h)?;
            Ok(u.port().map(|u| u.to_string()))
        } else {
            Err(ClientBuilderError::MissingHostUrl)
        }
    }

    fn host(&self) -> Option<&str> {
        todo!()
    }
}
