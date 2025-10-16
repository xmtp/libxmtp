use crate::error::GrpcBuilderError;
use http::Request;
use std::time::Duration;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic::{body::Body, client::GrpcService};
use tower::Service;

use std::task::{Context, Poll};

#[derive(Clone, Debug)]
pub struct NativeGrpcService {
    inner: Channel,
}

impl NativeGrpcService {
    pub fn new(
        host: String,
        limit: Option<u64>,
        is_secure: bool,
    ) -> Result<Self, GrpcBuilderError> {
        let channel = match is_secure {
            true => create_tls_channel(host, limit.unwrap_or(5000))?,
            false => apply_channel_options(Channel::from_shared(host)?, limit.unwrap_or(5000))
                .connect_lazy(),
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
pub fn create_tls_channel(address: String, limit: u64) -> Result<Channel, GrpcBuilderError> {
    let channel = apply_channel_options(Channel::from_shared(address)?, limit)
        .tls_config(ClientTlsConfig::new().with_enabled_roots())?
        .connect_lazy();
    Ok(channel)
}
