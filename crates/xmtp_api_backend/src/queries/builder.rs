use crate::{
    AuthCallback, AuthHandle, AuthMiddleware, BackendClient, ReadonlyClient, TrackedStatsClient,
    XmtpApiClient,
};
use std::sync::Arc;
use xmtp_api_grpc::GrpcClient;
use xmtp_proto::{
    api::ToBoxedClient,
    api_client::{ApiBuilder, NetConnectConfig},
    types::AppVersion,
};

#[derive(Default, Clone)]
pub struct MessageBackendBuilder {
    host: Option<String>,
    app_version: Option<AppVersion>,
    readonly: bool,
    auth_callback: Option<Arc<dyn AuthCallback>>,
    auth_handle: Option<AuthHandle>,
}

#[derive(Debug, thiserror::Error)]
pub enum MessageBackendBuilderError {
    #[error("backend URL is required")]
    MissingHost,
    #[error(transparent)]
    InvalidUrl(#[from] url::ParseError),
    #[error(transparent)]
    Grpc(#[from] xmtp_api_grpc::error::GrpcBuilderError),
}

impl MessageBackendBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn host(&mut self, host: impl AsRef<str>) -> &mut Self {
        self.host = Some(host.as_ref().into());
        self
    }
    pub fn app_version(&mut self, version: impl Into<AppVersion>) -> &mut Self {
        self.app_version = Some(version.into());
        self
    }
    pub fn readonly(&mut self, value: bool) -> &mut Self {
        self.readonly = value;
        self
    }
    pub fn maybe_auth_callback(&mut self, value: Option<Arc<dyn AuthCallback>>) -> &mut Self {
        self.auth_callback = value;
        self
    }
    pub fn maybe_auth_handle(&mut self, value: Option<AuthHandle>) -> &mut Self {
        self.auth_handle = value;
        self
    }
    pub fn build(&mut self) -> Result<XmtpApiClient, MessageBackendBuilderError> {
        let host = self
            .host
            .as_ref()
            .ok_or(MessageBackendBuilderError::MissingHost)?;
        let mut builder = GrpcClient::builder();
        builder.set_host(url::Url::parse(host)?);
        if let Some(version) = self.app_version.clone() {
            builder.set_app_version(version)?;
        }
        let client = builder.build()?;
        let client = if self.auth_callback.is_some() || self.auth_handle.is_some() {
            AuthMiddleware::new(client, self.auth_callback.clone(), self.auth_handle.clone())
                .arced()
        } else {
            client.arced()
        };
        let client = if self.readonly {
            ReadonlyClient { inner: client }.arced()
        } else {
            client
        };
        Ok(Arc::new(TrackedStatsClient::new(BackendClient::new(
            client,
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_proto::{api::ApiClientError, api_client::XmtpBackendClient};

    #[xmtp_common::test(unwrap_try = true)]
    async fn builder_requires_a_host_and_attaches_readonly_policy() {
        assert!(matches!(
            MessageBackendBuilder::default().build(),
            Err(MessageBackendBuilderError::MissingHost)
        ));
        let client = MessageBackendBuilder::default()
            .host(xmtp_configuration::backend_test_url())
            .readonly(true)
            .build()?;
        assert!(matches!(
            client.publish(Default::default()).await,
            Err(ApiClientError::WritesDisabled)
        ));
    }
}
