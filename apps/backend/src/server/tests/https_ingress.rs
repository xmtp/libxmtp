use super::{GrpcWebResponse, api, encode_frame, support};
use api::subscribe_static_response::Response as Frame;
use std::sync::Arc;
use support::{TestResult, TestServer, grpc_web};
use tokio::{net::TcpListener, task::JoinSet};
use tokio_rustls::TlsAcceptor;
use xmtp_common::time::{Duration, timeout};
use xmtp_mls_validation::test_utils::inline_welcome_envelope;

struct HttpsIngress {
    url: String,
    client: reqwest::Client,
    task: tokio::task::JoinHandle<TestResult>,
}

impl HttpsIngress {
    /// Terminate TLS and pass HTTP bytes unchanged to the backend.
    /// No gRPC conversion or complete-response buffer exists at this boundary.
    async fn start(backend: &str) -> TestResult<Self> {
        xmtp_cryptography::install_crypto_provider();
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
        let client = xmtp_common::http::client_builder()
            .add_root_certificate(reqwest::Certificate::from_der(cert.der())?)
            .default_headers(
                [
                    ("origin", "https://app.example"),
                    ("authorization", "Bearer test"),
                    ("x-app-version", "https-test"),
                    ("x-libxmtp-version", "https-test"),
                ]
                .into_iter()
                .map(|(name, value)| Ok((name.parse()?, value.parse()?)))
                .collect::<TestResult<reqwest::header::HeaderMap>>()?,
            )
            .http1_only()
            .build()?;
        let tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert.der().clone()],
                rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
            )?;
        let acceptor = TlsAcceptor::from(Arc::new(tls));
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("https://localhost:{}", listener.local_addr()?.port());
        let upstream = backend
            .strip_prefix("http://")
            .ok_or("expected HTTP backend")?
            .to_owned();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (socket, _) = accepted?;
                        let acceptor = acceptor.clone();
                        let upstream = upstream.clone();
                        connections.spawn(async move {
                            let mut tls = acceptor.accept(socket).await?;
                            let mut backend = tokio::net::TcpStream::connect(upstream).await?;
                            tokio::io::copy_bidirectional(&mut tls, &mut backend).await?;
                            Ok::<_, std::io::Error>(())
                        });
                    }
                    result = connections.join_next(), if !connections.is_empty() => {
                        let _ = result.ok_or("missing ingress task")??;
                    }
                }
            }
        });
        Ok(Self { url, client, task })
    }
}

impl Drop for HttpsIngress {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Require a frame before the response ends, so buffering fails this check.
async fn next_frame(
    response: &mut grpc_web::WebStream<api::SubscribeStaticResponse>,
) -> TestResult<Frame> {
    timeout(Duration::from_secs(5), response.stream.message())
        .await??
        .ok_or("stream ended")?
        .response
        .ok_or_else(|| "empty static response".into())
}

#[xmtp_common::timeout(std::time::Duration::from_secs(20))]
#[xmtp_common::test(unwrap_try = true)]
async fn https_passthrough_preserves_streaming_headers_and_status_details() {
    let server = TestServer::new(|_| {}).await?;
    let ingress = HttpsIngress::start(&server.url).await?;
    let path = format!(
        "{}/xmtp.backend.v1.SubscriptionService/SubscribeStatic",
        ingress.url
    );
    let preflight = ingress
        .client
        .request(reqwest::Method::OPTIONS, &path)
        .header("origin", "https://app.example")
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "authorization,content-type,x-app-version,x-libxmtp-version",
        )
        .send()
        .await?;
    assert!(preflight.status().is_success());
    let allowed = preflight.headers()["access-control-allow-headers"].to_str()?;
    for header in [
        "authorization",
        "content-type",
        "x-app-version",
        "x-libxmtp-version",
    ] {
        assert!(allowed.split(',').any(|value| value.trim() == header));
    }
    let envelope = inline_welcome_envelope([71; 32]);
    let parsed = xmtp_mls_validation::parse_envelope(envelope.clone())?;
    let topic = api::Topic {
        topic: parsed.topic.to_vec(),
    };
    let mut response = grpc_web::open(
        &ingress.client,
        &ingress.url,
        "/xmtp.backend.v1.SubscriptionService/SubscribeStatic",
        api::SubscribeStaticRequest {
            topics: vec![api::TopicQuery {
                topic: Some(topic.clone()),
                cursor: None,
            }],
        },
    )
    .await?;
    assert_eq!(response.headers["access-control-allow-origin"], "*");
    assert!(response.headers.contains_key("x-request-id"));
    let exposed = response.headers["access-control-expose-headers"].to_str()?;
    for header in [
        "grpc-status",
        "grpc-message",
        "grpc-status-details-bin",
        "x-request-id",
    ] {
        assert!(exposed.split(',').any(|value| value.trim() == header));
    }
    let Frame::Started(started) = next_frame(&mut response).await? else {
        panic!("first frame must be Started");
    };
    assert_eq!(started.targets.len(), 1);
    assert_eq!(started.targets[0].topic, Some(topic));
    assert_eq!(started.targets[0].through_sequence_id, 0);
    let meta = server.publish(vec![envelope.clone()]).await?.remove(0);
    loop {
        match next_frame(&mut response).await? {
            Frame::Messages(messages) => {
                assert_eq!(messages.envelopes.len(), 1);
                assert_eq!(messages.envelopes[0].envelope, Some(envelope));
                assert_eq!(messages.envelopes[0].meta, Some(meta));
                break;
            }
            Frame::Keepalive(_) => {}
            Frame::Started(_) => panic!("unexpected second Started"),
        }
    }
    drop(response);
    let response = ingress
        .client
        .post(format!(
            "{}/xmtp.backend.v1.PublishService/Publish",
            ingress.url
        ))
        .header("content-type", "application/grpc-web+proto")
        .body(encode_frame(api::PublishRequest {
            envelopes: vec![api::ClientEnvelope::default()],
        }))
        .send()
        .await?;
    let status = response.status();
    let headers = response.headers().clone();
    let response = GrpcWebResponse {
        status,
        headers,
        body: response.bytes().await?.to_vec(),
    };
    assert_eq!(
        response.grpc_status(),
        Some(tonic::Code::InvalidArgument as i32)
    );
    assert!(response.has_grpc_status_details());
    drop(ingress);
    server.stop().await?;
}
