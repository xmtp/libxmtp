use crate::{
    Backend,
    api::{
        configuration_service_server::ConfigurationServiceServer,
        identity_service_server::IdentityServiceServer,
        notification_service_server::NotificationServiceServer,
        publish_service_server::PublishServiceServer, query_service_server::QueryServiceServer,
        subscription_service_server::SubscriptionServiceServer,
    },
    config::{Config, MAX_CONFIGURATION_RESPONSE_BYTES},
    db::Store,
};
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{server::NamedService, transport::Server};
use tonic_web::GrpcWebLayer;
use tower_http::cors::{AllowHeaders, Any, CorsLayer};
use xmtp_id::scw_verifier::{
    CachedSmartContractSignatureVerifier, MultiSmartContractSignatureVerifier,
};

mod auth;
mod lifecycle;
pub(crate) mod telemetry;
#[cfg(test)]
mod tests;

/// Validate startup configuration and construct storage and signature services.
///
/// The primary database is migrated before the backend is returned. A verifier
/// cache is created with the configured non-zero capacity, and cryptography is
/// installed before any chain-RPC client is built. Auth keys load before storage;
/// an exhausted JWKS startup fetch returns a host-only error before binding.
pub async fn initialize(
    config: Config,
) -> Result<Backend, Box<dyn std::error::Error + Send + Sync>> {
    config.validate()?;
    xmtp_cryptography::install_crypto_provider();
    let push_senders = crate::push::channel::Senders::new(&config.push)?;
    // A disabled section loads no keys and fetches no JWKS. Only an enabled
    // one reaches key material at all.
    let auth = match config.auth.as_ref().filter(|auth| auth.is_enabled()) {
        Some(auth) => Some(std::sync::Arc::new(
            crate::auth::Authentication::initialize(auth).await?,
        )),
        None => None,
    };
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
    config
        .push
        .expires_at(store.clock_ns().await?)
        .map_err(|_| crate::config::ConfigError::Invalid {
            field: "push.recipient_ttl_seconds",
            reason: "expiry cannot be represented at the database clock",
        })?;
    let streams =
        crate::stream::StreamHub::start(store.primary.clone(), store.read.clone(), &config).await?;
    let mut backend = Backend::new(store, config, verifier);
    let push = &backend.config.push;
    if push.http.is_some() || push.apns.is_some() || push.fcm.is_some() {
        backend.push = Some(crate::push::PushHub::start(
            (*backend.store).clone(),
            &backend.config,
            streams.maintenance.clone(),
            push_senders,
        ));
    } else {
        crate::telemetry::push_dispatcher(false);
    }
    backend.streams = Some(streams);
    backend.auth = auth;
    // The key set is known only now, so the published settings are completed
    // here and then never change. The size bound is re-checked because a JWKS
    // key set is not visible to configuration validation.
    if let Some(auth) = &backend.auth {
        let response = backend
            .config
            .configuration_response(&auth.published_keys());
        crate::config::validate_configuration_size(&response)?;
        backend.configuration = Arc::new(response);
    }
    Ok(backend)
}

/// Server failures after the normal bounded drain.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("auth state does not match configuration; initialize the backend before serving")]
    AuthNotInitialized,
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),
    #[error("JWKS keys exceeded the maximum stale time")]
    JwksStale,
}

/// Configure gRPC, gRPC-Web, health, size limits, and graceful shutdown.
///
/// The service implementations share the supplied backend. The listener and
/// shutdown future belong to the caller, which controls when serving starts and
/// ends. Auth state must come from `initialize`; inconsistent state fails closed.
// implements: OPS-004, API-283, API-285
pub async fn serve(
    backend: Backend,
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), ServeError> {
    let expects_auth = backend
        .config
        .auth
        .as_ref()
        .is_some_and(crate::config::auth::AuthConfig::is_enabled);
    if expects_auth != backend.auth.is_some() {
        return Err(ServeError::AuthNotInitialized);
    }
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
    let notification = NotificationServiceServer::new(backend.clone())
        .max_decoding_message_size(receive)
        .max_encoding_message_size(send);
    // The one response this service sends is bounded by the startup check in
    // `validate_configuration_size`, not by the deployment's response budget.
    // A deployment free to publish a small `max_response_bytes` would otherwise
    // be unable to deliver a configuration its own validation accepted.
    let configuration = ConfigurationServiceServer::new(backend.clone())
        .max_decoding_message_size(receive)
        .max_encoding_message_size(MAX_CONFIGURATION_RESPONSE_BYTES);
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
        push: backend.push.clone(),
    };
    let incoming_lifecycle = lifecycle.clone();
    let incoming = TcpListenerStream::new(listener)
        .map(move |socket| socket.map(|socket| incoming_lifecycle.connection(socket)));
    let jwks_stale = Arc::new(AtomicBool::new(false));
    let mut refresh = tokio::task::JoinSet::new();
    if let Some(auth) = backend.auth.clone().filter(|auth| auth.jwks.is_some()) {
        let stale = jwks_stale.clone();
        refresh.spawn(async move {
            if let Some(source) = &auth.jwks {
                source.refresh(&auth.verifier.keys, auth.last_success).await;
                stale.store(true, Ordering::Release);
            }
        });
    }
    let stale = async {
        if refresh.is_empty() {
            std::future::pending::<()>().await;
        }
        let _ = refresh.join_next().await;
    };
    let shutdown = async {
        tokio::select! { biased; _ = stale => {}, _ = shutdown => {} }
    };
    let identifier: Arc<str> = Arc::from(backend.config.server.identifier());
    let auth_layer = tower::ServiceBuilder::new().option_layer(backend.auth.as_ref().map(|auth| {
        auth::AuthLayer {
            verifier: auth.verifier.clone(),
            identifier: identifier.clone(),
        }
    }));
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let serving = Server::builder()
        .accept_http1(true)
        .max_concurrent_streams(limits.max_http2_streams as u32)
        .layer(cors)
        .layer(telemetry::GrpcTelemetryLayer {
            enabled: backend.config.server.request_logger,
            identifier: identifier.clone(),
        })
        .layer(GrpcWebLayer::new())
        .layer(telemetry::GrpcStatusLayer)
        .layer(auth_layer)
        .layer(lifecycle::AdmissionLayer(lifecycle.clone()))
        .add_service(health)
        .add_service(configuration)
        .add_service(query)
        .add_service(publish)
        .add_service(identity)
        .add_service(subscription)
        .add_service(notification)
        .serve_with_incoming_shutdown(incoming, async {
            let _ = stopped.await;
        });
    tokio::pin!(serving);
    let result = tokio::select! {
        result = &mut serving => result,
        _ = shutdown => {
            let drain = xmtp_common::time::Duration::from_millis(backend.config.server.max_drain_duration_ms);
            let deadline = xmtp_common::time::Instant::now() + drain;
            if let Some(push) = &backend.push { push.stop(deadline); }
            guard.stop();
            report_health(&reporter, tonic_health::ServingStatus::NotServing).await;
            let _ = stop.send(());
            let draining = async {
                let (result, ()) = tokio::join!(&mut serving, async {
                    if let Some(push) = &backend.push { push.finished().await; }
                });
                result
            };
            match xmtp_common::time::timeout(deadline.saturating_duration_since(xmtp_common::time::Instant::now()), draining).await {
                Ok(result) => result,
                Err(_) => {
                    lifecycle.cancel();
                    if let Some(push) = &backend.push {
                        push.abort();
                        push.finished().await;
                    }
                    Ok(())
                },
            }
        }
    };
    if let Some(push) = &backend.push {
        push.stop(xmtp_common::time::Instant::now());
    }
    if jwks_stale.load(Ordering::Acquire) {
        Err(ServeError::JwksStale)
    } else {
        result.map_err(ServeError::from)
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
        ConfigurationServiceServer::<Backend>::NAME,
        QueryServiceServer::<Backend>::NAME,
        PublishServiceServer::<Backend>::NAME,
        IdentityServiceServer::<Backend>::NAME,
        SubscriptionServiceServer::<Backend>::NAME,
        NotificationServiceServer::<Backend>::NAME,
    ] {
        reporter.set_service_status(service, status).await;
    }
}
