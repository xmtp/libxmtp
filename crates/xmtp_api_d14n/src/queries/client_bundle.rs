use std::sync::Arc;

use crate::{
    AuthCallback, AuthHandle, MessageBackendBuilderError, MultiNodeClientBuilderError,
    ReadWriteClient, ReadonlyClient,
};
use derive_builder::Builder;
use http::{request, uri::PathAndQuery};
use prost::bytes::Bytes;
use xmtp_api_grpc::GrpcClient;
use xmtp_configuration::PAYER_WRITE_FILTER;
use xmtp_proto::{
    api::{ApiClientError, ArcClient, BytesStream, Client, IsConnectedCheck, ToBoxedClient},
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

pub struct ClientBundle {
    client: ArcClient,
    kind: ClientKind,
}

impl Clone for ClientBundle {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            kind: self.kind,
        }
    }
}

impl ClientBundle {
    pub fn builder() -> ClientBundleBuilder {
        ClientBundleBuilder::default()
    }
}

#[xmtp_common::async_trait]
impl Client for ClientBundle {
    async fn request(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError> {
        self.client.request(request, path, body).await
    }

    async fn stream(
        &self,
        request: request::Builder,
        path: PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        self.client.stream(request, path, body).await
    }
}

#[xmtp_common::async_trait]
impl IsConnectedCheck for ClientBundle {
    async fn is_connected(&self) -> bool {
        self.client.is_connected().await
    }
}

impl ClientBundle {
    pub fn new(client: ArcClient, kind: ClientKind) -> Self {
        Self { client, kind }
    }

    /// create a d14n client bundle
    pub fn d14n(client: ArcClient) -> Self {
        Self {
            client,
            kind: ClientKind::D14n,
        }
    }

    /// Create a v3 client bundle
    pub fn v3(client: ArcClient) -> Self {
        Self {
            client,
            kind: ClientKind::V3,
        }
    }

    /// Create a hybrid client
    pub fn hybrid(client: ArcClient) -> Self {
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

    fn inner_build_d14n(
        &mut self,
        gw_host: String,
        is_secure: bool,
        readonly: bool,
    ) -> Result<ClientBundle, MessageBackendBuilderError> {
        let Self {
            app_version,
            auth_callback,
            auth_handle,
            ..
        } = self.clone();

        let mut gateway_client_builder = GrpcClient::builder();
        gateway_client_builder.set_host(gw_host.to_string());
        gateway_client_builder.set_tls(is_secure);

        if let Some(version) = app_version {
            gateway_client_builder.set_app_version(version)?;
        }
        let gateway_client = gateway_client_builder.build()?;
        let gateway_client = if auth_callback.is_some() || auth_handle.is_some() {
            crate::AuthMiddleware::new(gateway_client, auth_callback, auth_handle).arced()
        } else {
            gateway_client.arced()
        };

        let mut multi_node = crate::middleware::MultiNodeClient::builder();
        let multi_node = multi_node.gateway_client(gateway_client.clone());
        let mut template = GrpcClient::builder();
        template.set_tls(is_secure);
        let multi_node = multi_node
            .node_client_template(template)
            .build()
            .map_err(MultiNodeClientBuilderError::from)?;

        if readonly {
            return Ok(ClientBundle::d14n(
                ReadonlyClient::builder().inner(multi_node).build()?.arced(),
            ));
        }
        let client = ReadWriteClient::builder()
            .read(multi_node)
            .write(gateway_client)
            .filter(PAYER_WRITE_FILTER)
            .build()?;

        Ok(ClientBundle::d14n(client.arced()))
    }

    /// build a client that is d14n only
    pub fn build_d14n(&mut self) -> Result<ClientBundle, MessageBackendBuilderError> {
        let Self {
            gateway_host,
            is_secure,
            ..
        } = self.clone();

        todo!()
    }

    pub fn build(&mut self) -> Result<ClientBundle, MessageBackendBuilderError> {
        let Self {
            v3_host,
            gateway_host,
            app_version,
            auth_callback,
            auth_handle,
            is_secure,
            readonly,
        } = self.clone();
        let v3_host = v3_host.ok_or(MessageBackendBuilderError::MissingV3Host)?;
        let is_secure = is_secure.unwrap_or_default();
        let readonly = readonly.unwrap_or_default();

        // implicitly use a d14n client
        if let Some(gateway) = gateway_host {
            self.inner_build_d14n(gateway, is_secure, readonly)
        } else {
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
    }
}
