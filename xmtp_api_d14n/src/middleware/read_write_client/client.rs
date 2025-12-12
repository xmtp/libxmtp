//! We define a very simple strategy for separating reads/writes for different
//! grpc calls.
//! If more control is required we could extend or modify this client implementation
//! to filter with regex, or let the consumer pass in a closure instead of a static
//! string filter.

use derive_builder::Builder;
use prost::bytes::Bytes;
use xmtp_proto::api::IsConnectedCheck;
use xmtp_proto::api::{ApiClientError, Client};

/// A client which holds two clients
/// and decides on a read/write strategy based on a given service str
/// if the query path contains a match for the given filter,
/// the client will write with the write client.
/// For all other queries it does a read.
#[derive(Debug, Builder, Default, Clone)]
#[builder(public)]
pub struct ReadWriteClient<Read, Write> {
    #[builder(public)]
    pub(super) read: Read,
    #[builder(public)]
    pub(super) write: Write,
    #[builder(setter(into), public)]
    pub(super) filter: String,
}

impl<Read: Clone, Write: Clone> ReadWriteClient<Read, Write> {
    pub fn builder() -> ReadWriteClientBuilder<Read, Write> {
        ReadWriteClientBuilder::default()
    }
}

#[xmtp_common::async_trait]
impl<Read, Write> Client for ReadWriteClient<Read, Write>
where
    Read: Client<Error = Write::Error, Stream = Write::Stream>,
    Write: Client,
{
    type Error = <Read as Client>::Error;

    type Stream = <Read as Client>::Stream;

    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        if path.path().contains(&self.filter) {
            self.write.request(request, path, body).await
        } else {
            self.read.request(request, path, body).await
        }
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        if path.path().contains(&self.filter) {
            self.write.stream(request, path, body).await
        } else {
            self.read.stream(request, path, body).await
        }
    }

    fn fake_stream(&self) -> http::Response<Self::Stream> {
        self.read.fake_stream()
    }
}

#[xmtp_common::async_trait]
impl<R, W> IsConnectedCheck for ReadWriteClient<R, W>
where
    R: IsConnectedCheck,
    W: IsConnectedCheck,
{
    async fn is_connected(&self) -> bool {
        // This implementation gives concurrent execution with early return.
        let to_result = |connected: bool| if connected { Ok(()) } else { Err(()) };
        let read = async { to_result(self.read.is_connected().await) };
        let write = async { to_result(self.write.is_connected().await) };
        let result = futures::future::try_join(read, write).await;
        result.is_ok()
    }
}

xmtp_common::if_test! {
    use derive_builder::UninitializedFieldError;
    use xmtp_proto::prelude::ApiBuilder;
    #[allow(clippy::unwrap_used)]
    impl<R, W> ReadWriteClientBuilder<R, W>
    where
        R: ApiBuilder,
        W: ApiBuilder,
    {
        pub(crate) fn build_builder(
            self,
        ) -> Result<ReadWriteClient<R::Output, W::Output>, UninitializedFieldError> {
            Ok(ReadWriteClient {
                read: <R as ApiBuilder>::build(
                    self.read
                        .ok_or(UninitializedFieldError::new("read"))
                        .unwrap(),
                )
                .unwrap(),
                write: <W as ApiBuilder>::build(
                    self.write
                        .ok_or(UninitializedFieldError::new("write"))
                        .unwrap(),
                )
                .unwrap(),
                filter: self
                    .filter
                    .ok_or(UninitializedFieldError::new("filter"))
                    .unwrap(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::d14n::{PublishClientEnvelopes, QueryEnvelope};

    use super::*;
    use rstest::*;

    use xmtp_proto::{
        api::{Query, mock::MockNetworkClient},
        types::TopicKind,
        xmtp::xmtpv4::envelopes::ClientEnvelope,
    };
    const FILTER: &str = "xmtp.xmtpv4.payer_api.PayerApi";
    type MockClient = ReadWriteClient<MockNetworkClient, MockNetworkClient>;

    #[fixture]
    fn rw() -> MockClient {
        ReadWriteClient {
            read: MockNetworkClient::default(),
            write: MockNetworkClient::default(),
            filter: FILTER.to_string(),
        }
    }

    #[rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_writes_when_matches(mut rw: MockClient) {
        rw.write
            .expect_request()
            .times(1)
            .returning(|_, _, _| Ok(http::Response::new(vec![].into())));
        let mut e = PublishClientEnvelopes::builder()
            .envelope(ClientEnvelope::default())
            .build()?;
        e.query(&rw).await?;
    }

    #[rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_reads_when_matches(mut rw: MockClient) {
        rw.read
            .expect_request()
            .times(1)
            .returning(|_, _, _| Ok(http::Response::new(vec![].into())));
        let mut e = QueryEnvelope::builder()
            .topic(TopicKind::GroupMessagesV1.create(vec![]))
            .last_seen(Default::default())
            .limit(0)
            .build()?;
        e.query(&rw).await?;
    }
}
