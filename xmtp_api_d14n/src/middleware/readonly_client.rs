//! We define a very simple strategy for disabling writes on certain clients.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReadonlyClientBuilderError {
    #[error("Inner client is not provided")]
    MissingInner,
}

xmtp_common::if_test! {
    mod test;
}

use prost::bytes::Bytes;
use xmtp_proto::api::IsConnectedCheck;
use xmtp_proto::api::{ApiClientError, Client};

const DENY: &[&str] = &[
    "UploadKeyPackage",
    "RevokeInstallation",
    "BatchPublishCommitLog",
    "SendWelcomeMessages",
    "RegisterInstallation",
    "PublishIdentityUpdate",
];

/// A client that will error on requests that write to the network.
#[derive(Debug, Default)]
pub struct ReadonlyClient<Client> {
    pub(super) inner: Client,
}

impl<C> ReadonlyClient<C> {
    pub fn builder() -> ReadonlyClientBuilder<C> {
        ReadonlyClientBuilder::default()
    }
}

pub struct ReadonlyClientBuilder<C> {
    inner: Option<C>,
}

impl<C> Default for ReadonlyClientBuilder<C> {
    fn default() -> Self {
        Self { inner: None }
    }
}

impl<C> ReadonlyClientBuilder<C> {
    pub fn new(client: C) -> Self {
        Self {
            inner: Some(client),
        }
    }

    pub fn inner(mut self, client: C) -> Self {
        self.inner = Some(client);
        self
    }

    pub fn build(self) -> Result<ReadonlyClient<C>, ReadonlyClientBuilderError> {
        let Some(inner) = self.inner else {
            return Err(ReadonlyClientBuilderError::MissingInner);
        };
        Ok(ReadonlyClient { inner })
    }
}

#[xmtp_common::async_trait]
impl<C> Client for ReadonlyClient<C>
where
    C: Client,
{
    type Error = <C as Client>::Error;
    type Stream = <C as Client>::Stream;

    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        let p = path.path();
        if DENY.iter().any(|d| p.contains(d)) {
            return Err(ApiClientError::WritesDisabled);
        }

        self.inner.request(request, path, body).await
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        let p = path.path();
        if DENY.iter().any(|d| p.contains(d)) {
            return Err(ApiClientError::WritesDisabled);
        }

        self.inner.stream(request, path, body).await
    }
}

#[xmtp_common::async_trait]
impl<C> IsConnectedCheck for ReadonlyClient<C>
where
    C: IsConnectedCheck,
{
    async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }
}

xmtp_common::if_test! {
    use derive_builder::UninitializedFieldError;
    use xmtp_proto::prelude::ApiBuilder;
    #[allow(clippy::unwrap_used)]
    impl<C> ReadonlyClientBuilder<C>
    where
        C: ApiBuilder,
    {
        pub(crate) fn build_builder(
            self,
        ) -> Result<ReadonlyClient<C::Output>, UninitializedFieldError> {
            Ok(ReadonlyClient {
                inner: <C as ApiBuilder>::build(
                    self.inner
                        .ok_or(UninitializedFieldError::new("read"))
                        .unwrap(),
                )
                .unwrap(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{d14n::PublishClientEnvelopes, v3::PublishIdentityUpdate};

    use super::*;
    use rstest::*;

    use xmtp_proto::{
        api::{Query, mock::MockNetworkClient},
        xmtp::xmtpv4::envelopes::ClientEnvelope,
    };
    type MockClient = ReadonlyClient<MockNetworkClient>;

    #[fixture]
    fn ro() -> MockClient {
        ReadonlyClient {
            inner: MockNetworkClient::default(),
        }
    }

    #[rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_forwards_to_inner(mut ro: MockClient) {
        ro.inner
            .expect_request()
            .times(1)
            .returning(|_, _, _| Ok(http::Response::new(vec![].into())));
        let mut e = PublishClientEnvelopes::builder()
            .envelope(ClientEnvelope::default())
            .build()?;
        e.query(&ro).await?;
    }

    #[rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_errors_on_write(ro: MockClient) {
        let mut e = PublishIdentityUpdate::builder().build()?;
        let result = e.query(&ro).await;
        assert!(matches!(result, Err(ApiClientError::WritesDisabled)));
    }
}
