use std::{error::Error, time::Duration};

use crate::{MessageBackendBuilderError, MiddlewareBuilder, ReadWriteClient};
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
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
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
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
#[derive(Builder, Clone, Debug)]
#[builder(public, name = "ClientBundleBuilder", build_fn(skip))]
struct __ClientBundleBuilder {
    #[builder(setter(into))]
    app_version: AppVersion,
    #[builder(setter(into))]
    v3_host: String,
    #[builder(setter(into, strip_option))]
    gateway_host: Option<String>,
    is_secure: bool,
}

impl ClientBundleBuilder {
    pub fn maybe_gateway_host<U: Into<String>>(&mut self, host: Option<U>) -> &mut Self {
        if let Some(h) = host {
            self.gateway_host = Some(Some(h.into()));
        }
        self
    }

    pub fn build(&mut self) -> Result<ClientBundle<GrpcError>, MessageBackendBuilderError> {
        let Self {
            v3_host,
            gateway_host,
            app_version,
            is_secure,
        } = self.clone();
        let v3_host = v3_host.ok_or(MessageBackendBuilderError::MissingV3Host)?;
        let gateway_host = gateway_host.unwrap_or_default();
        let is_secure = is_secure.unwrap_or_default();

        // implicitly use a d14n client
        if let Some(gateway) = gateway_host {
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
            let rw = ReadWriteClient::builder()
                .read(multi_node)
                .write(gateway_client)
                .filter(PAYER_WRITE_FILTER)
                .build()?;
            let client = rw.arced();
            Ok(ClientBundle::d14n(client))
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
