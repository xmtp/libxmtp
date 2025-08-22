use crate::error::GrpcBuilderError;
use http::Request;
use std::time::Duration;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic::{body::Body, client::GrpcService};
use tower::Service;
use tracing::Instrument;

use std::task::{Context, Poll};

#[derive(Clone, Debug)]
pub struct NativeGrpcService {
    inner: Channel,
}

impl NativeGrpcService {
    pub async fn new(
        host: String,
        limit: Option<u64>,
        is_secure: bool,
    ) -> Result<Self, GrpcBuilderError> {
        let channel = match is_secure {
            true => create_tls_channel(host, limit.unwrap_or(5000)).await?,
            false => {
                apply_channel_options(Channel::from_shared(host)?, limit.unwrap_or(5000))
                    .connect()
                    .await?
            }
        };

        Ok(Self { inner: channel })
    }
}

impl Service<Request<Body>> for NativeGrpcService {
    type Response = <Channel as Service<Request<Body>>>::Response;
    type Error = <Channel as GrpcService<Body>>::Error;
    type Future = <Channel as GrpcService<Body>>::Future;

    fn poll_ready(&mut self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        <Channel as Service<Request<Body>>>::poll_ready(&mut self.inner, ctx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        <Channel as Service<Request<Body>>>::call(&mut self.inner, request)
    }
}

pub(crate) fn apply_channel_options(endpoint: Endpoint, limit: u64) -> Endpoint {
    endpoint
        // Purpose: This setting controls the size of the initial connection-level flow control window for HTTP/2, which is the underlying protocol for gRPC.
        // Functionality: Flow control in HTTP/2 manages how much data can be in flight on the network. Setting the initial connection window size to (1 << 31) - 1 (the maximum possible value for a 32-bit integer, which is 2,147,483,647 bytes) essentially allows the client to receive a very large amount of data from the server before needing to acknowledge receipt and permit more data to be sent. This can be particularly useful in high-latency networks or when transferring large amounts of data.
        // Impact: Increasing the window size can improve throughput by allowing more data to be in transit at a time, but it may also increase memory usage and can potentially lead to inefficient use of bandwidth if the network is unreliable.
        .initial_connection_window_size(Some((1 << 31) - 1))
        // Purpose: Configures whether the client should send keep-alive pings to the server when the connection is idle.
        // Functionality: When set to true, this option ensures that periodic pings are sent on an idle connection to keep it alive and detect if the server is still responsive.
        // Impact: This helps maintain active connections, particularly through NATs, load balancers, and other middleboxes that might drop idle connections. It helps ensure that the connection is promptly usable when new requests need to be sent.
        .keep_alive_while_idle(true)
        // Purpose: Sets the maximum amount of time the client will wait for a connection to be established.
        // Functionality: If a connection cannot be established within the specified duration, the attempt is aborted and an error is returned.
        // Impact: This setting prevents the client from waiting indefinitely for a connection to be established, which is crucial in scenarios where rapid failure detection is necessary to maintain responsiveness or to quickly fallback to alternative services or retry logic.
        .connect_timeout(Duration::from_secs(10))
        // Purpose: Configures the TCP keep-alive interval for the socket connection.
        // Functionality: This setting tells the operating system to send TCP keep-alive probes periodically when no data has been transferred over the connection within the specified interval.
        // Impact: Similar to the gRPC-level keep-alive, this helps keep the connection alive at the TCP layer and detect broken connections. It's particularly useful for detecting half-open connections and ensuring that resources are not wasted on unresponsive peers.
        .tcp_keepalive(Some(Duration::from_secs(16)))
        // Purpose: Sets a maximum duration for the client to wait for a response to a request.
        // Functionality: If a response is not received within the specified timeout, the request is canceled and an error is returned.
        // Impact: This is critical for bounding the wait time for operations, which can enhance the predictability and reliability of client interactions by avoiding indefinitely hanging requests.
        .timeout(Duration::from_secs(120))
        // Purpose: Specifies how long the client will wait for a response to a keep-alive ping before considering the connection dead.
        // Functionality: If a ping response is not received within this duration, the connection is presumed to be lost and is closed.
        // Impact: This setting is crucial for quickly detecting unresponsive connections and freeing up resources associated with them. It ensures that the client has up-to-date information on the status of connections and can react accordingly.
        .keep_alive_timeout(Duration::from_secs(10))
        .http2_keep_alive_interval(Duration::from_secs(16))
        .rate_limit(limit, Duration::from_secs(60))
}

#[tracing::instrument(level = "trace", skip_all)]
pub async fn create_tls_channel(address: String, limit: u64) -> Result<Channel, GrpcBuilderError> {
    let span = tracing::debug_span!("grpc_connect", address);
    let channel = apply_channel_options(Channel::from_shared(address)?, limit)
        .tls_config(ClientTlsConfig::new().with_enabled_roots())?
        .connect()
        .instrument(span)
        .await?;

    Ok(channel)
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use super::*;
    use futures::Stream;
    use http::Uri;
    use tokio_stream::StreamExt as _;
    use tonic::transport::Server;
    use tower::service_fn;
    use xmtp_proto::{
        api::Client,
        mls_v1::{
            mls_api_server::{MlsApi, MlsApiServer},
            welcome_message, BatchPublishCommitLogRequest, BatchQueryCommitLogRequest,
            BatchQueryCommitLogResponse, FetchKeyPackagesRequest, FetchKeyPackagesResponse,
            GetIdentityUpdatesRequest, GetIdentityUpdatesResponse, GroupMessage,
            QueryGroupMessagesRequest, QueryGroupMessagesResponse, QueryWelcomeMessagesRequest,
            QueryWelcomeMessagesResponse, RegisterInstallationRequest,
            RegisterInstallationResponse, RevokeInstallationRequest, SendGroupMessagesRequest,
            SendWelcomeMessagesRequest, SubscribeGroupMessagesRequest,
            SubscribeWelcomeMessagesRequest, UploadKeyPackageRequest, WelcomeMessage,
        },
    };

    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct OldWelcomeMessage {
        #[prost(oneof = "OldVersion", tags = "1")]
        pub version: Option<OldVersion>,
    }

    #[derive(Clone, PartialEq, Eq, Hash, prost::Oneof)]
    pub enum OldVersion {
        #[prost(message, tag = "1")]
        V1(OldV1),
    }

    #[derive(Clone, PartialEq, Eq, Hash, prost::Message)]
    pub struct OldV1 {
        #[prost(uint64, tag = "1")]
        pub id: u64,
        #[prost(uint64, tag = "2")]
        pub created_ns: u64,
        #[prost(bytes = "vec", tag = "3")]
        pub installation_key: ::prost::alloc::vec::Vec<u8>,
        #[prost(bytes = "vec", tag = "4")]
        pub data: ::prost::alloc::vec::Vec<u8>,
        #[prost(bytes = "vec", tag = "5")]
        pub hpke_public_key: ::prost::alloc::vec::Vec<u8>,
    }

    type ResponseStream<T> = Pin<Box<dyn Stream<Item = Result<T, tonic::Status>> + Send>>;
    struct MockMlsApi;
    #[async_trait::async_trait]
    impl MlsApi for MockMlsApi {
        async fn send_group_messages(
            &self,
            _request: tonic::Request<SendGroupMessagesRequest>,
        ) -> std::result::Result<tonic::Response<pbjson_types::Empty>, tonic::Status> {
            unimplemented!();
        }

        async fn send_welcome_messages(
            &self,
            _request: tonic::Request<SendWelcomeMessagesRequest>,
        ) -> std::result::Result<tonic::Response<pbjson_types::Empty>, tonic::Status> {
            unimplemented!()
        }

        async fn register_installation(
            &self,
            _request: tonic::Request<RegisterInstallationRequest>,
        ) -> std::result::Result<tonic::Response<RegisterInstallationResponse>, tonic::Status>
        {
            unimplemented!()
        }

        async fn upload_key_package(
            &self,
            _request: tonic::Request<UploadKeyPackageRequest>,
        ) -> std::result::Result<tonic::Response<pbjson_types::Empty>, tonic::Status> {
            unimplemented!()
        }

        async fn fetch_key_packages(
            &self,
            _request: tonic::Request<FetchKeyPackagesRequest>,
        ) -> std::result::Result<tonic::Response<FetchKeyPackagesResponse>, tonic::Status> {
            unimplemented!()
        }

        async fn revoke_installation(
            &self,
            _request: tonic::Request<RevokeInstallationRequest>,
        ) -> std::result::Result<tonic::Response<pbjson_types::Empty>, tonic::Status> {
            unimplemented!()
        }

        async fn get_identity_updates(
            &self,
            _request: tonic::Request<GetIdentityUpdatesRequest>,
        ) -> std::result::Result<tonic::Response<GetIdentityUpdatesResponse>, tonic::Status>
        {
            unimplemented!()
        }

        async fn query_group_messages(
            &self,
            _request: tonic::Request<QueryGroupMessagesRequest>,
        ) -> std::result::Result<tonic::Response<QueryGroupMessagesResponse>, tonic::Status>
        {
            unimplemented!()
        }

        async fn query_welcome_messages(
            &self,
            _request: tonic::Request<QueryWelcomeMessagesRequest>,
        ) -> std::result::Result<tonic::Response<QueryWelcomeMessagesResponse>, tonic::Status>
        {
            unimplemented!()
        }

        type SubscribeGroupMessagesStream = ResponseStream<GroupMessage>;
        async fn subscribe_group_messages(
            &self,
            _request: tonic::Request<SubscribeGroupMessagesRequest>,
        ) -> std::result::Result<tonic::Response<Self::SubscribeGroupMessagesStream>, tonic::Status>
        {
            unimplemented!()
        }

        type SubscribeWelcomeMessagesStream = ResponseStream<WelcomeMessage>;

        async fn subscribe_welcome_messages(
            &self,
            _request: tonic::Request<SubscribeWelcomeMessagesRequest>,
        ) -> std::result::Result<tonic::Response<Self::SubscribeWelcomeMessagesStream>, tonic::Status>
        {
            let repeat = std::iter::repeat(())
                .enumerate()
                .map(|(i, _)| generate_welcome(i as u64));
            // creating infinite stream with requested message
            let mut stream =
                Box::pin(tokio_stream::iter(repeat).throttle(Duration::from_millis(200)));
            // spawn and channel are required if you want handle "disconnect" functionality
            // the `out_stream` will not be polled after client disconnect
            let (tx, rx) = tokio::sync::mpsc::channel(128);
            tokio::spawn(async move {
                while let Some(item) = stream.next().await {
                    match tx.send(Result::<_, tonic::Status>::Ok(item)).await {
                        Ok(_) => {
                            // item (server response) was queued to be send to client
                        }
                        Err(_item) => {
                            // output_stream was build from rx and both are dropped
                            break;
                        }
                    }
                }
                println!("\tclient disconnected");
            });

            let output_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
            Ok(tonic::Response::new(
                Box::pin(output_stream) as Self::SubscribeWelcomeMessagesStream
            ))
        }

        async fn batch_publish_commit_log(
            &self,
            _request: tonic::Request<BatchPublishCommitLogRequest>,
        ) -> std::result::Result<tonic::Response<pbjson_types::Empty>, tonic::Status> {
            unimplemented!()
        }

        async fn batch_query_commit_log(
            &self,
            _request: tonic::Request<BatchQueryCommitLogRequest>,
        ) -> std::result::Result<tonic::Response<BatchQueryCommitLogResponse>, tonic::Status>
        {
            unimplemented!()
        }
    }

    fn generate_welcome(id: u64) -> WelcomeMessage {
        WelcomeMessage {
            version: Some(welcome_message::Version::V1(welcome_message::V1 {
                id,
                created_ns: xmtp_common::rand_u64(),
                installation_key: xmtp_common::rand_vec::<32>(),
                data: xmtp_common::rand_vec::<256>(),
                hpke_public_key: xmtp_common::rand_vec::<256>(),
                wrapper_algorithm: 1,
                welcome_metadata: xmtp_common::rand_vec::<32>(),
            })),
        }
    }
    // wasm can't spawn a mock server
    // therefore this test is only in native
    //
    // todo: in the future, if there is a greater need for a mock mls api,
    // we could move this setup to a fn
    // reference: https://github.com/hyperium/tonic/blob/master/examples/src/mock/mock.rs
    #[tokio::test]
    async fn grpc_client_is_forwards_compatible() {
        let (client, server) = tokio::io::duplex(1024);
        tokio::spawn(async move {
            Server::builder()
                .add_service(MlsApiServer::new(MockMlsApi))
                .serve_with_incoming(tokio_stream::once(Ok::<_, std::io::Error>(server)))
                .await
        });

        // Move client to an option so we can _move_ the inner value
        // on the first attempt to connect. All other attempts will fail.
        let mut client = Some(client);
        let channel = Endpoint::try_from("http://[::]:50051")
            .unwrap()
            .connect_with_connector(service_fn(move |_: Uri| {
                let client = client.take();

                async move {
                    if let Some(client) = client {
                        Ok(hyper_util::rt::TokioIo::new(client))
                    } else {
                        Err(std::io::Error::other("Client already taken"))
                    }
                }
            }))
            .await
            .unwrap();
        let client = super::super::client::GrpcClient::new(
            channel,
            "".try_into().unwrap(),
            "".try_into().unwrap(),
        );
        let path = xmtp_proto::path_and_query::<SubscribeWelcomeMessagesRequest>();
        let path: http::uri::PathAndQuery = path.parse().unwrap();
        let stream = client
            .stream(http::request::Builder::new(), path, vec![].into())
            .await
            .unwrap()
            .into_body();
        futures::pin_mut!(stream);
        let bytes = stream.next().await.unwrap().unwrap();
        let message: OldWelcomeMessage = prost::Message::decode(&mut bytes.to_vec().as_slice()).unwrap();
        println!("{:?}", message);
    }
}
