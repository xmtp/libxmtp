use crate::{
    Backend,
    api::{
        identity_service_server::IdentityServiceServer,
        publish_service_server::PublishServiceServer, query_service_server::QueryServiceServer,
        subscription_service_server::SubscriptionServiceServer,
    },
    config::Config,
    db::Store,
};
use std::{collections::HashMap, num::NonZeroUsize};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic_web::GrpcWebLayer;
use tower_http::cors::{AllowHeaders, Any, CorsLayer};
use xmtp_id::scw_verifier::{
    CachedSmartContractSignatureVerifier, MultiSmartContractSignatureVerifier,
};

pub async fn initialize(
    config: Config,
) -> Result<Backend, Box<dyn std::error::Error + Send + Sync>> {
    config.validate()?;
    xmtp_cryptography::install_crypto_provider();
    let routes = config
        .chains
        .iter()
        .map(|(chain, url)| Ok((chain.clone(), url.parse()?)))
        .collect::<Result<HashMap<_, _>, url::ParseError>>()?;
    let verifier = MultiSmartContractSignatureVerifier::new(routes)?;
    let capacity = NonZeroUsize::new(config.validation.max_scw_cache_entries)
        .ok_or("signature cache cannot be empty")?;
    let verifier = CachedSmartContractSignatureVerifier::new(verifier, capacity)?;
    let store = Store::connect(&config).await?;
    Ok(Backend::new(store, config, verifier))
}

pub async fn serve(
    backend: Backend,
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), tonic::transport::Error> {
    let limits = &backend.config.limits;
    let receive = limits.max_request_bytes;
    let send = limits.max_response_bytes;
    let query = QueryServiceServer::new(backend.clone())
        .max_decoding_message_size(receive)
        .max_encoding_message_size(send);
    let publish = PublishServiceServer::new(backend.clone())
        .max_decoding_message_size(receive)
        .max_encoding_message_size(send);
    let identity = IdentityServiceServer::new(backend.clone())
        .max_decoding_message_size(receive)
        .max_encoding_message_size(send);
    let subscription = SubscriptionServiceServer::new(backend.clone())
        .max_decoding_message_size(receive)
        .max_encoding_message_size(send);
    let (reporter, health) = tonic_health::server::health_reporter();
    reporter.set_serving::<QueryServiceServer<Backend>>().await;
    reporter
        .set_serving::<PublishServiceServer<Backend>>()
        .await;
    reporter
        .set_serving::<IdentityServiceServer<Backend>>()
        .await;
    reporter
        .set_service_status("", tonic_health::ServingStatus::Serving)
        .await;
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(AllowHeaders::mirror_request())
        .expose_headers([
            "grpc-status".parse().expect("static header"),
            "grpc-message".parse().expect("static header"),
            "grpc-status-details-bin".parse().expect("static header"),
        ]);
    Server::builder()
        .accept_http1(true)
        .max_concurrent_streams(limits.max_http2_streams as u32)
        .layer(cors)
        .layer(GrpcWebLayer::new())
        .add_service(health)
        .add_service(query)
        .add_service(publish)
        .add_service(identity)
        .add_service(subscription)
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), shutdown)
        .await
}
