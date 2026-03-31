use std::sync::Arc;

use crate::{
    AuthCallback, AuthHandle, MessageBackendBuilderError, ReadWriteClient, ReadonlyClient,
};
use derive_builder::Builder;
use xmtp_api_grpc::GrpcClient;
use xmtp_configuration::{PAYER_WRITE_FILTER, XmtpEnv};
use xmtp_proto::{
    api::{ArcClient, ToBoxedClient},
    prelude::{ApiBuilder, NetConnectConfig},
    types::AppVersion,
};

#[derive(Clone)]
#[non_exhaustive]
pub enum ClientBundle {
    D14n {
        client: ArcClient,
        app_version: Option<xmtp_proto::types::AppVersion>,
    },
    V3(ArcClient),
    Migration {
        v3: ArcClient,
        xmtpd: ArcClient,
    },
}

impl std::fmt::Display for ClientBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ClientBundle::*;
        match self {
            D14n { .. } => write!(f, "D14n"),
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
    pub fn d14n(client: ArcClient, app_version: Option<xmtp_proto::types::AppVersion>) -> Self {
        ClientBundle::D14n {
            client,
            app_version,
        }
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
            Self::D14n { .. } => None,
            Self::V3(v3) | Self::Migration { v3, .. } => Some(v3.clone()),
        }
    }

    pub fn get_d14n(&self) -> Option<ArcClient> {
        match self {
            Self::D14n { client, .. } => Some(client.clone()),
            Self::Migration { xmtpd, .. } => Some(xmtpd.clone()),
            Self::V3(_) => None,
        }
    }
}

// we aren't using any of the generated build fns by derive_builder here
// instead we are just using it to generate some setters on the impl for us.
#[derive(Builder, Clone)]
#[builder(public, name = "ClientBundleBuilder", build_fn(skip))]
struct __ClientBundleBuilder {
    #[builder(setter(into), default)]
    app_version: AppVersion,
    #[builder(setter(into))]
    v3_host: String,
    #[builder(setter(into))]
    gateway_host: String,
    #[builder(setter(into))]
    auth_callback: Arc<dyn AuthCallback>,
    #[builder(setter(into))]
    auth_handle: AuthHandle,
    readonly: bool,
}

// getters
impl ClientBundleBuilder {
    pub fn get_v3_host(&self) -> Option<&String> {
        self.v3_host.as_ref()
    }

    pub fn get_gateway_host(&self) -> Option<&String> {
        self.gateway_host.as_ref()
    }

    pub fn get_app_version(&self) -> Option<&AppVersion> {
        self.app_version.as_ref()
    }
}

// setters
impl ClientBundleBuilder {
    /// Set the v3 host if `host` is `Some`
    /// Overwrites any value that may already be set
    pub fn maybe_v3_host<U: Into<String>>(&mut self, host: Option<U>) -> &mut Self {
        self.v3_host = host.map(Into::into).or_else(|| self.v3_host.take());
        self
    }

    /// Set the gateway host if `host` is `Some`
    /// Overwrites any value that may already be set
    pub fn maybe_gateway_host<U: Into<String>>(&mut self, host: Option<U>) -> &mut Self {
        self.gateway_host = host.map(Into::into).or_else(|| self.gateway_host.take());
        self
    }

    /// Set the auth callback if `callback` is `Some`
    /// Overwrites any value that may already be set
    pub fn maybe_auth_callback(&mut self, callback: Option<Arc<dyn AuthCallback>>) -> &mut Self {
        self.auth_callback = callback.or_else(|| self.auth_callback.take());
        self
    }

    /// Set the handle if `handle` is `Some`
    /// Overwrites any value that may already be set
    pub fn maybe_auth_handle(&mut self, handle: Option<AuthHandle>) -> &mut Self {
        self.auth_handle = handle.or_else(|| self.auth_handle.take());
        self
    }

    pub fn maybe_app_version<V: Into<AppVersion>>(&mut self, version: Option<V>) -> &mut Self {
        self.app_version = version.map(Into::into).or_else(|| self.app_version.take());
        self
    }
    /// Specify a fallback value for the V3 Host. Only used as a fallback.
    /// `v3_host()` always takes precedence. Never overwrites `v3_host` if already set.
    pub fn env(&mut self, env: XmtpEnv) -> &mut Self {
        // self.v3_host is prioritized if already set over default Env
        self.v3_host = self
            .v3_host
            .take()
            .or_else(|| env.default_api_url().map(Into::into));
        self
    }

    fn inner_build_d14n(
        &mut self,
    ) -> Result<(ArcClient, Option<xmtp_proto::types::AppVersion>), MessageBackendBuilderError>
    {
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
        gateway_client_builder.set_host(
            gw_host
                .parse()
                .map_err(|e| MessageBackendBuilderError::invalid_url(e, gw_host.clone()))?,
        );

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
        if let Some(ref version) = app_version {
            template.set_app_version(version.clone())?;
        }
        let multi_node = multi_node.node_client_template(template).build()?;

        if readonly {
            return Ok((
                ReadonlyClient::builder().inner(multi_node).build()?.arced(),
                app_version,
            ));
        }

        let client = ReadWriteClient::builder()
            .read(multi_node)
            .write(gateway_client)
            .filter(PAYER_WRITE_FILTER)
            .build()?;

        Ok((client.arced(), app_version))
    }

    /// build a client that is d14n only
    /// Errors:
    /// * if the gateway_host is missing.
    pub fn build_d14n(&mut self) -> Result<ClientBundle, MessageBackendBuilderError> {
        let (client, app_version) = self.inner_build_d14n()?;
        Ok(ClientBundle::d14n(client, app_version))
    }

    fn inner_build_v3(&mut self) -> Result<ArcClient, MessageBackendBuilderError> {
        let v3_host = self
            .v3_host
            .as_ref()
            .ok_or(MessageBackendBuilderError::MissingV3Host)?;
        let readonly = self.readonly.unwrap_or_default();
        let mut v3_client = GrpcClient::builder();
        v3_client.set_host(
            v3_host
                .parse()
                .map_err(|e| MessageBackendBuilderError::invalid_url(e, v3_host.clone()))?,
        );
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

    /// build a client that is v3 only
    /// Errors:
    /// * if v3_host is missing.
    pub fn build_v3(&mut self) -> Result<ClientBundle, MessageBackendBuilderError> {
        Ok(ClientBundle::v3(self.inner_build_v3()?))
    }

    /// Build the default client
    /// The default client will migrate to v3 on cutover
    /// Errors if either V3 or Gateway host are missing
    pub fn build(&mut self) -> Result<ClientBundle, MessageBackendBuilderError> {
        let (d14n, _app_version) = self.inner_build_d14n()?;
        let v3 = self.inner_build_v3()?;
        Ok(ClientBundle::migration(v3, d14n))
    }

    /// If a gateway is present, build a d14n-only client
    /// otherwise build a v3 client.
    /// Errors if V3 host is missing
    pub fn build_optional_d14n(&mut self) -> Result<ClientBundle, MessageBackendBuilderError> {
        let Self {
            gateway_host: ref gw,
            ..
        } = self.clone();
        if gw.is_some() {
            let (client, app_version) = self.inner_build_d14n()?;
            Ok(ClientBundle::d14n(client, app_version))
        } else {
            Ok(ClientBundle::v3(self.inner_build_v3()?))
        }
    }
}

#[cfg(test)]
mod tests {
    use xmtp_configuration::GrpcUrlsDev;

    use super::*;

    #[xmtp_common::test]
    fn env_cannot_be_overridden_by_none() {
        let mut builder = ClientBundle::builder();
        builder
            .env(XmtpEnv::Dev)
            .maybe_v3_host(Option::<String>::None);
        assert!(builder.v3_host.is_some());
        assert_eq!(builder.v3_host, Some(GrpcUrlsDev::NODE.to_string()))
    }
}
