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
use tonic::{server::NamedService, transport::Server};
use tonic_web::GrpcWebLayer;
use tower_http::cors::{AllowHeaders, Any, CorsLayer};
use xmtp_id::scw_verifier::{
    CachedSmartContractSignatureVerifier, MultiSmartContractSignatureVerifier,
};

mod lifecycle;
pub(crate) mod telemetry;
#[cfg(test)]
mod tests;

/// Validate startup configuration and construct storage and signature services.
///
/// The primary database is migrated before the backend is returned. A verifier
/// cache is created with the configured non-zero capacity, and cryptography is
/// installed before any chain-RPC client is built.
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
    config.retention.validate_at(store.clock_ns().await?)?;
    let streams =
        crate::stream::StreamHub::start(store.primary.clone(), store.read.clone(), &config).await?;
    let mut backend = Backend::new(store, config, verifier);
    crate::telemetry::spawn_sampler(
        std::sync::Arc::downgrade(&backend.store),
        streams.fetches.clone(),
        crate::stream::FETCH_WORKERS,
        tokio::runtime::Handle::current(),
    );
    backend.streams = Some(streams);
    Ok(backend)
}

/// Configure gRPC, gRPC-Web, health, size limits, and graceful shutdown.
///
/// The service implementations share the supplied backend. The listener and
/// shutdown future belong to the caller, which controls when serving starts and
/// ends.
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
    report_health(&reporter, tonic_health::ServingStatus::Serving).await;
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(AllowHeaders::mirror_request())
        .expose_headers([
            "grpc-status".parse().expect("static header"),
            "grpc-message".parse().expect("static header"),
            "grpc-status-details-bin".parse().expect("static header"),
            "x-request-id".parse().expect("static header"),
        ]);
    use futures::StreamExt;
    let lifecycle = lifecycle::Lifecycle::new();
    let guard = lifecycle::ShutdownGuard {
        lifecycle: lifecycle.clone(),
        streams: backend.streams.clone(),
    };
    let incoming_lifecycle = lifecycle.clone();
    let incoming = TcpListenerStream::new(listener)
        .map(move |socket| socket.map(|socket| incoming_lifecycle.connection(socket)));
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let serving = Server::builder()
        .accept_http1(true)
        .max_concurrent_streams(limits.max_http2_streams as u32)
        .layer(cors)
        .layer(telemetry::GrpcTelemetryLayer(
            backend.config.server.request_logger,
        ))
        .layer(GrpcWebLayer::new())
        .layer(telemetry::GrpcStatusLayer)
        .layer(lifecycle::AdmissionLayer(lifecycle.clone()))
        .add_service(health)
        .add_service(query)
        .add_service(publish)
        .add_service(identity)
        .add_service(subscription)
        .serve_with_incoming_shutdown(incoming, async {
            let _ = stopped.await;
        });
    tokio::pin!(serving);
    tokio::select! {
        result = &mut serving => result,
        _ = shutdown => {
            guard.stop();
            report_health(&reporter, tonic_health::ServingStatus::NotServing).await;
            let _ = stop.send(());
            let drain = xmtp_common::time::Duration::from_millis(backend.config.server.max_drain_duration_ms);
            match xmtp_common::time::timeout(drain, &mut serving).await {
                Ok(result) => result,
                Err(_) => { lifecycle.cancel(); Ok(()) },
            }
        }
    }
}

/// Keep aggregate health and each advertised RPC service in the same lifecycle
/// state. Named health watchers must see shutdown before connections drain.
async fn report_health(
    reporter: &tonic_health::server::HealthReporter,
    status: tonic_health::ServingStatus,
) {
    crate::telemetry::ready(status == tonic_health::ServingStatus::Serving);
    for service in [
        "",
        QueryServiceServer::<Backend>::NAME,
        PublishServiceServer::<Backend>::NAME,
        IdentityServiceServer::<Backend>::NAME,
        SubscriptionServiceServer::<Backend>::NAME,
    ] {
        reporter.set_service_status(service, status).await;
    }
}
