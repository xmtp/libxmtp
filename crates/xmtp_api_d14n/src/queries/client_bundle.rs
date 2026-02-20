use std::{error::Error, sync::Arc, time::Duration};

use crate::{
    AuthCallback, AuthHandle, MessageBackendBuilderError, MiddlewareBuilder, ReadWriteClient,
    ReadonlyClient,
};
use derive_builder::Builder;
use http::{request, uri::PathAndQuery};
use prost::bytes::Bytes;
use xmtp_api_grpc::{GrpcClient, error::GrpcError};
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_configuration::{MULTI_NODE_TIMEOUT_MS, PAYER_WRITE_FILTER};
use xmtp_proto::{
    api::{ApiClientError, ArcClient, Client, IsConnectedCheck, ToBoxedClient},
    prelude::{ApiBuilder, NetConnectConfig},
    types::AppVersion,
};

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum ClientKind {
    D14n,
    V3,
    Hybrid,
}
impl std::fmt::Display for ClientKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ClientKind::*;
        match self {
            D14n => write!(f, "D14n"),
            V3 => write!(f, "V3"),
            Hybrid => write!(f, "Hybrid"),
        }
    }
}

pub struct ClientBundle<Err> {
    client: ArcClient<Err>,
    kind: ClientKind,
}

impl<Err> Clone for ClientBundle<Err> {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            kind: self.kind,
        }
    }
}

impl ClientBundle<()> {
    pub fn builder() -> ClientBundleBuilder {
        ClientBundleBuilder::default()
    }
}

#[xmtp_common::async_trait]
impl<Err> Client for ClientBundle<Err>
where
    Err: Error + MaybeSend + MaybeSync + 'static,
{
    type Error = Err;

    type Stream = <ArcClient<Err> as Client>::Stream;

    async fn request(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        self.client.request(request, path, body).await
    }

    async fn stream(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        self.client.stream(request, path, body).await
    }

    fn fake_stream(&self) -> http::Response<Self::Stream> {
        self.client.fake_stream()
    }
}

#[xmtp_common::async_trait]
impl<Err> IsConnectedCheck for ClientBundle<Err> {
    async fn is_connected(&self) -> bool {
        self.client.is_connected().await
    }
}

impl<Err> ClientBundle<Err> {
    pub fn new(client: ArcClient<Err>, kind: ClientKind) -> Self {
        Self { client, kind }
    }

    /// create a d14n client bundle
    pub fn d14n(client: ArcClient<Err>) -> Self {
        Self {
            client,
            kind: ClientKind::D14n,
        }
    }

    /// Create a v3 client bundle
    pub fn v3(client: ArcClient<Err>) -> Self {
        Self {
            client,
            kind: ClientKind::V3,
        }
    }

    /// Create a hybrid client
    pub fn hybrid(client: ArcClient<Err>) -> Self {
        Self {
            client,
            kind: ClientKind::Hybrid,
        }
    }

    pub fn kind(&self) -> &ClientKind {
        &self.kind
    }
}

// we aren't using any of the generated build fns by derive_builder here
// instead we are just using it to generate the setters on the impl for us.
#[derive(Builder, Clone)]
#[builder(public, name = "ClientBundleBuilder", build_fn(skip))]
struct __ClientBundleBuilder {
    #[builder(setter(into))]
    app_version: AppVersion,
    #[builder(setter(into))]
    v3_host: String,
    #[builder(setter(into))]
    gateway_host: String,
    #[builder(setter(into))]
    auth_callback: Arc<dyn AuthCallback>,
    #[builder(setter(into))]
    auth_handle: AuthHandle,
    is_secure: bool,
    readonly: bool,
}

impl ClientBundleBuilder {
    pub fn maybe_v3_host<U: Into<String>>(&mut self, host: Option<U>) -> &mut Self {
        self.v3_host = host.map(Into::into);
        self
    }

    pub fn maybe_gateway_host<U: Into<String>>(&mut self, host: Option<U>) -> &mut Self {
        self.gateway_host = host.map(Into::into);
        self
    }

    pub fn maybe_auth_callback(&mut self, callback: Option<Arc<dyn AuthCallback>>) -> &mut Self {
        self.auth_callback = callback;
        self
    }

    pub fn maybe_auth_handle(&mut self, handle: Option<AuthHandle>) -> &mut Self {
        self.auth_handle = handle;
        self
    }

    pub fn build(&mut self) -> Result<ClientBundle<GrpcError>, MessageBackendBuilderError> {
        let Self {
            v3_host,
            gateway_host,
            app_version,
            auth_callback,
            auth_handle,
            is_secure,
            readonly,
        } = self.clone();
        let is_secure = is_secure.unwrap_or_default();
        let readonly = readonly.unwrap_or_default();

        match (v3_host, gateway_host) {
            // D14n mode: gateway_host is set (v3_host is ignored)
            (_, Some(gateway)) => {
                let mut gateway_client_builder = GrpcClient::builder();
                gateway_client_builder.set_host(gateway.to_string());
                gateway_client_builder.set_tls(is_secure);

                if let Some(version) = app_version {
                    gateway_client_builder.set_app_version(version)?;
                }

                let mut multi_node = crate::middleware::MultiNodeClientBuilder::default();
                multi_node.set_timeout(Duration::from_millis(MULTI_NODE_TIMEOUT_MS))?;
                multi_node.set_gateway_builder(gateway_client_builder.clone())?;
                let mut template = GrpcClient::builder();
                template.set_tls(is_secure);
                multi_node.set_node_client_builder(template)?;

                let gateway_client = gateway_client_builder.build()?;
                let multi_node = multi_node.build()?;

                let client = if auth_callback.is_some() || auth_handle.is_some() {
                    let auth =
                        crate::AuthMiddleware::new(gateway_client, auth_callback, auth_handle);
                    let client = ReadWriteClient::builder()
                        .read(multi_node)
                        .write(auth)
                        .filter(PAYER_WRITE_FILTER)
                        .build()?;
                    if readonly {
                        ReadonlyClient::builder().inner(client).build()?.arced()
                    } else {
                        client.arced()
                    }
                } else {
                    let client = ReadWriteClient::builder()
                        .read(multi_node)
                        .write(gateway_client)
                        .filter(PAYER_WRITE_FILTER)
                        .build()?;
                    if readonly {
                        ReadonlyClient::builder().inner(client).build()?.arced()
                    } else {
                        client.arced()
                    }
                };

                Ok(ClientBundle::d14n(client))
            }
            // V3 mode: only v3_host is set
            (Some(v3_host), None) => {
                let mut v3_client = GrpcClient::builder();
                v3_client.set_host(v3_host.to_string());
                v3_client.set_tls(is_secure);
                if let Some(ref version) = app_version {
                    v3_client.set_app_version(version.clone())?;
                }

                let v3_client = v3_client.build()?;
                let client = v3_client.arced();
                Ok(ClientBundle::v3(client))
            }
            // Neither host provided
            (None, None) => Err(MessageBackendBuilderError::MissingHost),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_build_v3_only() {
        let bundle = ClientBundle::builder()
            .v3_host("http://localhost:5050")
            .is_secure(false)
            .build()?;

        assert!(matches!(bundle.kind(), ClientKind::V3));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_build_d14n_with_gateway_only() {
        let bundle = ClientBundle::builder()
            .gateway_host("http://localhost:5050")
            .is_secure(false)
            .build()?;

        assert!(matches!(bundle.kind(), ClientKind::D14n));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_build_d14n_with_both_hosts() {
        let bundle = ClientBundle::builder()
            .v3_host("http://localhost:5050")
            .gateway_host("http://localhost:6060")
            .is_secure(false)
            .build()?;

        // When gateway_host is provided, D14n mode is used regardless of v3_host
        assert!(matches!(bundle.kind(), ClientKind::D14n));
    }

    #[xmtp_common::test]
    async fn test_build_no_hosts_fails() {
        let result = ClientBundle::builder().is_secure(false).build();

        match result {
            Err(MessageBackendBuilderError::MissingHost) => {} // expected
            Err(other) => panic!("Expected MissingHost error, got: {other:?}"),
            Ok(_) => panic!("Expected error when neither host is provided"),
        }
    }
}
