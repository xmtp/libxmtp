pub mod error;
pub mod grpc_api_helper;
pub mod grpc_client;
pub use error::*;
mod identity;

pub const LOCALHOST_ADDRESS: &str = "http://localhost:5556";
pub const DEV_ADDRESS: &str = "https://grpc.dev.xmtp.network:443";
pub const GRPC_PAYLOAD_LIMIT: usize = 1024 * 1024 * 25;

pub use grpc_api_helper::{Client, GroupMessageStream, WelcomeMessageStream};
use std::time::Duration;
use tonic::transport::{Channel, ClientTlsConfig};
use tracing::Instrument;

#[tracing::instrument(level = "trace", skip_all)]
pub async fn create_tls_channel(address: String, limit: u64) -> Result<Channel, GrpcBuilderError> {
    let span = tracing::debug_span!("grpc_connect", address);
    if let Err(err) = rustls::crypto::ring::default_provider().install_default() {
        tracing::warn!("CryptoProvider was already installed: {:?}", err);
    }
    let channel = Channel::from_shared(address)?
        .rate_limit(limit, Duration::from_secs(60))
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
        .tcp_keepalive(Some(Duration::from_secs(15)))
        // Purpose: Sets a maximum duration for the client to wait for a response to a request.
        // Functionality: If a response is not received within the specified timeout, the request is canceled and an error is returned.
        // Impact: This is critical for bounding the wait time for operations, which can enhance the predictability and reliability of client interactions by avoiding indefinitely hanging requests.
        .timeout(Duration::from_secs(120))
        // Purpose: Specifies how long the client will wait for a response to a keep-alive ping before considering the connection dead.
        // Functionality: If a ping response is not received within this duration, the connection is presumed to be lost and is closed.
        // Impact: This setting is crucial for quickly detecting unresponsive connections and freeing up resources associated with them. It ensures that the client has up-to-date information on the status of connections and can react accordingly.
        .keep_alive_timeout(Duration::from_secs(25))
        .tls_config(ClientTlsConfig::new().with_enabled_roots())?
        .connect()
        .instrument(span)
        .await?;

    Ok(channel)
}
#[cfg(test)]
pub mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use xmtp_proto::api_client::ApiBuilder;
    use xmtp_proto::xmtp::message_api::v1::{Envelope, PublishRequest};

    // Return the json serialization of an Envelope with bytes
    pub fn test_envelope(topic: String) -> Envelope {
        let time_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        Envelope {
            timestamp_ns: time_since_epoch.as_nanos() as u64,
            content_topic: topic,
            message: vec![65],
        }
    }

    #[tokio::test]
    async fn metadata_test() {
        let mut client = Client::builder();
        client.set_host(DEV_ADDRESS.to_string());
        client.set_tls(true);
        let app_version = "test/1.0.0".to_string();
        let libxmtp_version = "0.0.1".to_string();
        client.set_app_version(app_version.clone()).unwrap();
        client.set_libxmtp_version(libxmtp_version.clone()).unwrap();
        let client = client.build().await.unwrap();
        let request = client.build_request(PublishRequest { envelopes: vec![] });

        assert_eq!(
            request
                .metadata()
                .get("x-app-version")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
            app_version
        );
        assert_eq!(
            request
                .metadata()
                .get("x-libxmtp-version")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
            libxmtp_version
        );
    }
}
