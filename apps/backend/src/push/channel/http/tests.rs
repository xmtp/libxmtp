use super::*;
use crate::{db::PushChannel, push::channel::DeliveryConfig, test_support::TestResult};
use parking_lot::Mutex;
use std::{
    collections::{HashMap, VecDeque},
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::mpsc,
    task::{JoinHandle, JoinSet},
};
use xmtp_common::time::{Duration, Instant, timeout};

struct FixedResolver {
    addresses: Vec<SocketAddr>,
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl Resolver for FixedResolver {
    async fn resolve(&self, _: &str, _: u16) -> std::io::Result<Vec<SocketAddr>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.addresses.clone())
    }
}

pub(crate) struct Request {
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub(crate) struct Webhook {
    pub url: String,
    pub sender: Arc<HttpSender>,
    pub requests: mpsc::UnboundedReceiver<Request>,
    task: JoinHandle<TestResult>,
}

impl Webhook {
    pub async fn start(statuses: Vec<u16>) -> TestResult<Self> {
        xmtp_cryptography::install_crypto_provider();
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["push.invalid".into()])?;
        let root = reqwest::Certificate::from_der(cert.der())?;
        let tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert.der().clone()],
                rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
            )?;
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let url = format!("https://push.invalid:{}/hook", address.port());
        let sender = Arc::new(HttpSender {
            allow_private: true,
            allowed_domains: None,
            resolver: Arc::new(FixedResolver {
                addresses: vec![address],
                calls: AtomicUsize::new(0),
            }),
            trusted_root: Some(root),
        });
        let statuses = Arc::new(Mutex::new(VecDeque::from(statuses)));
        let (sent, requests) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (socket, _) = accepted?;
                        let acceptor = acceptor.clone();
                        let statuses = statuses.clone();
                        let sent = sent.clone();
                        connections.spawn(async move {
                            let mut socket = acceptor.accept(socket).await?;
                            let mut bytes = Vec::new();
                            let header_end = loop {
                                let mut part = [0; 1024];
                                let count = socket.read(&mut part).await?;
                                if count == 0 { return Err("webhook request ended".into()); }
                                bytes.extend_from_slice(&part[..count]);
                                if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") { break end + 4; }
                            };
                            let text = std::str::from_utf8(&bytes[..header_end])?;
                            let headers: HashMap<_, _> = text.lines().skip(1).filter_map(|line| line.split_once(':'))
                                .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string())).collect();
                            let length: usize = headers.get("content-length").ok_or("missing body length")?.parse()?;
                            while bytes.len() < header_end + length {
                                let mut part = [0; 1024];
                                let count = socket.read(&mut part).await?;
                                if count == 0 { return Err("webhook body ended".into()); }
                                bytes.extend_from_slice(&part[..count]);
                            }
                            sent.send(Request { headers, body: bytes[header_end..header_end + length].to_vec() })?;
                            let status = statuses.lock().pop_front().unwrap_or(200);
                            let response = format!("HTTP/1.1 {status} Scripted\r\ncontent-length: 0\r\nconnection: close\r\nlocation: /redirected\r\n\r\n");
                            socket.write_all(response.as_bytes()).await?;
                            socket.shutdown().await?;
                            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
                        });
                    },
                    result = connections.join_next(), if !connections.is_empty() => {
                        result.ok_or("missing webhook task")???;
                    },
                }
            }
        });
        Ok(Self {
            url,
            sender,
            requests,
            task,
        })
    }
}

impl Drop for Webhook {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn delivery(url: String) -> Delivery {
    Delivery {
        payload: xmtp_push_types::PushPayload::new(&[1, 2, 3], 9_007_199_254_740_993),
        config: DeliveryConfig {
            secret_hash: vec![9; 32],
            recipient_id: vec![1, 1],
            channel: PushChannel::Http,
            delivery: url,
            signing_key: Some(vec![7; 32]),
        },
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn signature_matches_an_independent_fixed_vector() {
    let body = body(&delivery("https://unused.invalid".into()))?;
    assert_eq!(
        std::str::from_utf8(&body)?,
        r#"{"topic":"AQID","sequence_id":"9007199254740993","recipient_id":"0101"}"#
    );
    assert_eq!(
        signature(&[7; 32], "fixed-id", "1700000000", &body),
        "v1,IszeBLRjxb7spJ5RLi+KW0w/aK3Buioq9E6jl3MKWZw="
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn tls_requests_use_pinned_dns_and_fresh_signed_headers_without_secrets() {
    let mut webhook = Webhook::start(vec![204, 201]).await?;
    let delivery = delivery(webhook.url.clone());
    let mut ids = Vec::new();
    for _ in 0..2 {
        assert_eq!(webhook.sender.send(&delivery).await, Outcome::Delivered);
        let request = webhook.requests.recv().await?;
        assert_eq!(request.headers.get("content-type")?, "application/json");
        let id = request.headers.get("webhook-id")?;
        uuid::Uuid::parse_str(id)?;
        ids.push(id.clone());
        let timestamp = request.headers.get("webhook-timestamp")?;
        let seconds: i64 = timestamp.parse()?;
        assert!((xmtp_common::time::now_secs() - seconds).abs() <= 2);
        assert_eq!(
            request.headers.get("webhook-signature")?,
            &signature(&[7; 32], id, timestamp, &request.body)
        );
        assert_eq!(request.body, body(&delivery)?);
        let bytes = String::from_utf8(request.body)?;
        assert!(!bytes.contains(&STANDARD.encode([7; 32])));
        assert!(!bytes.contains(&hex::encode([7; 32])));
        assert_eq!(
            request.headers.get("host")?,
            &webhook
                .url
                .trim_start_matches("https://")
                .trim_end_matches("/hook")
        );
    }
    assert_ne!(ids[0], ids[1]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn invalid_delivery_urls_and_missing_signing_keys_send_no_request() {
    let mut webhook = Webhook::start(vec![]).await?;
    for url in [
        "not a URL",
        "http://push.invalid/hook",
        "https://user@push.invalid/hook",
        "https://user:password@push.invalid/hook",
        "https://:password@push.invalid/hook",
    ] {
        assert_eq!(
            webhook.sender.send(&delivery(url.into())).await,
            Outcome::Rejected
        );
    }
    let mut unsigned = delivery(webhook.url.clone());
    unsigned.config.signing_key = None;
    assert_eq!(webhook.sender.send(&unsigned).await, Outcome::Rejected);
    assert!(webhook.requests.try_recv().is_err());
}

#[xmtp_common::test(unwrap_try = true)]
async fn send_time_domain_allowlist_rechecks_registered_destinations() {
    let mut webhook = Webhook::start(vec![204, 204]).await?;
    let delivery = delivery(webhook.url.clone());

    Arc::get_mut(&mut webhook.sender)?.allowed_domains = Some(vec![]);
    assert_eq!(webhook.sender.send(&delivery).await, Outcome::Delivered);
    webhook.requests.recv().await?;

    Arc::get_mut(&mut webhook.sender)?.allowed_domains = Some(vec!["*.INVALID".into()]);
    assert_eq!(webhook.sender.send(&delivery).await, Outcome::Delivered);
    webhook.requests.recv().await?;

    Arc::get_mut(&mut webhook.sender)?.allowed_domains = Some(vec!["other.invalid".into()]);
    assert_eq!(webhook.sender.send(&delivery).await, Outcome::Rejected);
    assert!(webhook.requests.try_recv().is_err());
}

#[xmtp_common::test(unwrap_try = true)]
async fn empty_dns_answer_is_transient_and_private_ip_literals_are_rejected() {
    let resolver = Arc::new(FixedResolver {
        addresses: Vec::new(),
        calls: AtomicUsize::new(0),
    });
    let sender = HttpSender {
        allow_private: false,
        allowed_domains: None,
        resolver: resolver.clone(),
        trusted_root: None,
    };
    assert_eq!(
        sender
            .send(&delivery("https://empty.invalid/hook".into()))
            .await,
        Outcome::Transient { retry_after: None }
    );
    for url in ["https://127.0.0.1/hook", "https://[::1]/hook"] {
        assert_eq!(sender.send(&delivery(url.into())).await, Outcome::Rejected);
    }
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn response_classes_and_redirects_use_the_provider_contract() {
    let cases = [
        (200, Outcome::Delivered),
        (299, Outcome::Delivered),
        (302, Outcome::Rejected),
        (400, Outcome::Rejected),
        (404, Outcome::GoneTransient),
        (410, Outcome::GoneTransient),
        (429, Outcome::Rejected),
        (500, Outcome::Transient { retry_after: None }),
        (503, Outcome::Transient { retry_after: None }),
    ];
    let mut webhook = Webhook::start(cases.iter().map(|(status, _)| *status).collect()).await?;
    for (_, expected) in cases {
        assert_eq!(
            webhook.sender.send(&delivery(webhook.url.clone())).await,
            expected
        );
        webhook.requests.recv().await?;
    }
    assert!(
        webhook.requests.try_recv().is_err(),
        "redirect caused a second request"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn send_time_dns_rejects_private_addresses_and_checks_every_answer() {
    let resolver = Arc::new(FixedResolver {
        addresses: vec!["1.1.1.1:443".parse()?, "169.254.169.254:443".parse()?],
        calls: AtomicUsize::new(0),
    });
    let sender = HttpSender {
        allow_private: false,
        allowed_domains: None,
        resolver: resolver.clone(),
        trusted_root: None,
    };
    for _ in 0..2 {
        assert_eq!(
            sender
                .send(&delivery("https://formerly-public.invalid/hook".into()))
                .await,
            Outcome::Rejected
        );
    }
    assert_eq!(
        resolver.calls.load(Ordering::SeqCst),
        2,
        "DNS results were reused across attempts"
    );
}

struct StalledResolver;
#[async_trait::async_trait]
impl Resolver for StalledResolver {
    async fn resolve(&self, _: &str, _: u16) -> std::io::Result<Vec<SocketAddr>> {
        std::future::pending().await
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn one_deadline_includes_stalled_dns() {
    let sender = HttpSender {
        allow_private: true,
        allowed_domains: None,
        resolver: Arc::new(StalledResolver),
        trusted_root: None,
    };
    let start = Instant::now();
    let outcome = timeout(
        ATTEMPT_TIMEOUT + Duration::from_secs(1),
        sender.send(&delivery("https://stalled.invalid".into())),
    )
    .await?;
    assert_eq!(outcome, Outcome::Transient { retry_after: None });
    assert!(start.elapsed() >= ATTEMPT_TIMEOUT);
}

#[xmtp_common::test(unwrap_try = true)]
async fn webhook_logs_do_not_contain_recipient_fields_or_signing_keys() {
    use tracing::instrument::WithSubscriber;
    let webhook = Webhook::start(vec![200]).await?;
    let mut delivery = delivery(webhook.url.clone());
    delivery.config.recipient_id = vec![0x8d; 32];
    delivery.config.signing_key = Some(vec![0x7e; 32]);
    delivery.payload = xmtp_push_types::PushPayload::new(&[0x9a; 17], 1);
    let capture = xmtp_logging::test_logging::LogCapture::new(xmtp_logging::Level::Trace);
    let outcome = async {
        tracing::info!(target: "xmtp_backend::push", "push privacy capture active");
        webhook.sender.send(&delivery).await
    }
    .with_subscriber(capture.dispatch())
    .await;
    assert_eq!(outcome, Outcome::Delivered);
    let output = capture.output();
    assert!(output.contains("push privacy capture active"));
    for forbidden in [
        delivery.config.delivery.clone(),
        hex::encode(&delivery.config.recipient_id),
        STANDARD.encode(delivery.config.signing_key.as_deref()?),
        hex::encode(delivery.config.signing_key.as_deref()?),
        delivery.payload.topic,
    ] {
        assert!(
            !output.contains(&forbidden),
            "log contains private recipient data"
        );
    }
}
