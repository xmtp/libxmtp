use std::sync::Arc;

use crate::{
    AuthCallback, AuthHandle, MessageBackendBuilderError, ReadWriteClient, ReadonlyClient,
};
use derive_builder::Builder;
use xmtp_api_grpc::GrpcClient;
use xmtp_configuration::PAYER_WRITE_FILTER;
use xmtp_proto::{
    api::{ArcClient, ToBoxedClient},
    prelude::{ApiBuilder, NetConnectConfig},
    types::AppVersion,
};

#[derive(Clone)]
#[non_exhaustive]
pub enum ClientBundle {
    D14n(ArcClient),
    V3(ArcClient),
    Migration { v3: ArcClient, xmtpd: ArcClient },
}

impl std::fmt::Display for ClientBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ClientBundle::*;
        match self {
            D14n(_) => write!(f, "D14n"),
            V3(_) => write!(f, "V3"),
            Migration { .. } => write!(f, "Migration"),
        }
    }
}

impl std::fmt::Debug for ClientBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self) // delegate to the display impl
    }
}

impl ClientBundle {
    pub fn builder() -> ClientBundleBuilder {
        ClientBundleBuilder::default()
    }
}

impl ClientBundle {
    /// create a d14n client bundle
    pub fn d14n(client: ArcClient) -> Self {
        ClientBundle::D14n(client)
    }

    /// Create a v3 client bundle
    pub fn v3(client: ArcClient) -> Self {
        ClientBundle::V3(client)
    }

    /// Create a migration client
    pub fn migration(v3: ArcClient, xmtpd: ArcClient) -> Self {
        ClientBundle::Migration { v3, xmtpd }
    }

    pub fn get_v3(&self) -> Option<ArcClient> {
        match self {
            Self::D14n(_) => None,
            Self::V3(v3) | Self::Migration { v3, .. } => Some(v3.clone()),
        }
    }

    pub fn get_d14n(&self) -> Option<ArcClient> {
        match self {
            Self::D14n(xmtpd) | Self::Migration { xmtpd, .. } => Some(xmtpd.clone()),
            Self::V3(_) => None,
        }
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
        is_secure: bool,
    ) -> Result<ArcClient, MessageBackendBuilderError> {
        let Self {
            app_version,
            auth_callback,
            auth_handle,
            ..
        } = self.clone();
        let gw_host = self
            .gateway_host
            .as_ref()
            .ok_or(MessageBackendBuilderError::MissingGatewayHost)?;
        let readonly = self.readonly.unwrap_or_default();

        let mut gateway_client_builder = GrpcClient::builder();
        gateway_client_builder.set_host(gw_host.to_string());
        gateway_client_builder.set_tls(is_secure);

        if let Some(ref version) = app_version {
            gateway_client_builder.set_app_version(version.clone())?;
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
        if let Some(ref version) = app_version {
            template.set_app_version(version.clone())?;
        }
        let multi_node = multi_node.node_client_template(template).build()?;

        if readonly {
            return Ok(ReadonlyClient::builder().inner(multi_node).build()?.arced());
        }

        let client = ReadWriteClient::builder()
            .read(multi_node)
            .write(gateway_client)
            .filter(PAYER_WRITE_FILTER)
            .build()?;

        Ok(client.arced())
    }

    /// build a client that is d14n only
    /// Errors:
    /// * if the gateway_host is missing.
    pub fn build_d14n(&mut self) -> Result<ClientBundle, MessageBackendBuilderError> {
        let is_secure = self.is_secure.unwrap_or_default();
        Ok(ClientBundle::d14n(self.inner_build_d14n(is_secure)?))
    }

    fn inner_build_v3(&mut self, is_secure: bool) -> Result<ArcClient, MessageBackendBuilderError> {
        let v3_host = self
            .v3_host
            .as_ref()
            .ok_or(MessageBackendBuilderError::MissingV3Host)?;
        let readonly = self.readonly.unwrap_or_default();
        let mut v3_client = GrpcClient::builder();
        v3_client.set_host(v3_host.to_string());
        v3_client.set_tls(is_secure);
        if let Some(ref version) = self.app_version {
            v3_client.set_app_version(version.clone())?;
        }
        let v3_client = v3_client.build()?;
        if readonly {
            Ok(ReadonlyClient::builder().inner(v3_client).build()?.arced())
        } else {
            Ok(v3_client.arced())
        }
    }

    /// build a client that is d14n only
    /// Errors:
    /// * if the gateway_host is missing.
    pub fn build_v3(&mut self) -> Result<ClientBundle, MessageBackendBuilderError> {
        let is_secure = self.is_secure.unwrap_or_default();
        Ok(ClientBundle::v3(self.inner_build_v3(is_secure)?))
    }

    /// Build the default client
    /// The default client will migrate to v3 on cutover
    pub fn build(&mut self) -> Result<ClientBundle, MessageBackendBuilderError> {
        let Self { is_secure, .. } = self.clone();
        let is_secure = is_secure.unwrap_or_default();
        let d14n = self.inner_build_d14n(is_secure)?;
        let v3 = self.inner_build_v3(is_secure)?;
        Ok(ClientBundle::migration(v3, d14n))
    }
}
