//! We define a very simple strategy for disabling writes on certain clients.

mod test;

use derive_builder::Builder;
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
#[derive(Debug, Builder, Default, Clone)]
#[builder(public)]
pub struct ReadonlyClient<Client> {
    #[builder(public)]
    pub(super) inner: Client,
}

impl<C: Clone> ReadonlyClient<C> {
    pub fn builder() -> ReadonlyClientBuilder<C> {
        ReadonlyClientBuilder::default()
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
    use crate::v3::PublishIdentityUpdate;

    use super::*;
    use rstest::*;

    use xmtp_proto::api::{Query, mock::MockNetworkClient};
    type MockClient = ReadonlyClient<MockNetworkClient>;

    #[fixture]
    fn ro() -> MockClient {
        ReadonlyClient {
            inner: MockNetworkClient::default(),
        }
    }

    #[rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_errors_on_write(ro: MockClient) {
        let mut e = PublishIdentityUpdate::builder().build()?;
        let result = e.query(&ro).await;
        assert!(matches!(result, Err(ApiClientError::WritesDisabled)));
    }
}
