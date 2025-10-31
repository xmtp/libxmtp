//! We define a very simple strategy for separating reads/writes for different
//! grpc calls.
//! If more control is required we could extend or modify this client implementation
//! to filter with regex, or let the consumer pass in a closure instead of a static
//! string filter.

use derive_builder::Builder;
use prost::bytes::Bytes;
use xmtp_proto::api::{ApiClientError, Client};

/// A client which holds two clients
/// and decides on a read/write strategy based on a given service str
/// if the query path contains a match for the given filter,
/// the client will write with the write client.
/// For all other queries it does a read.
#[derive(Debug, Builder, Default, Clone)]
pub struct ReadWriteClient<Read, Write> {
    pub(super) read: Read,
    pub(super) write: Write,
    #[builder(setter(into))]
    filter: String,
}

impl<Read: Clone, Write: Clone> ReadWriteClient<Read, Write> {
    pub fn builder() -> ReadWriteClientBuilder<Read, Write> {
        ReadWriteClientBuilder::default()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
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
}
