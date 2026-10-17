//! Scripted local providers. Drop cancels the listener and every connection.

use super::TestResult;
use bytes::Bytes;
use futures::{StreamExt, stream};
use http_body_util::{BodyExt, Full, StreamBody, combinators::BoxBody};
use hyper::{
    Request, Response,
    body::{Frame, Incoming},
    service::service_fn,
};
use hyper_util::rt::{TokioExecutor, TokioIo};
use parking_lot::Mutex;
use std::{collections::VecDeque, convert::Infallible, sync::Arc};
use tokio::{
    net::TcpListener,
    task::{JoinHandle, JoinSet},
};
use xmtp_common::time::Duration;

#[derive(Clone, Copy)]
pub(crate) enum Protocol {
    Http1,
    Http2Tls,
}

#[derive(Clone)]
pub(crate) struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
    pub headers: Vec<(&'static str, String)>,
    pub delay: Duration,
    pub stall_headers: bool,
    pub stall_body: bool,
}

impl Reply {
    pub fn json(status: u16, body: serde_json::Value) -> Self {
        Self {
            status,
            body: body.to_string().into_bytes(),
            headers: Vec::new(),
            delay: Duration::ZERO,
            stall_headers: false,
            stall_body: false,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Observed {
    pub method: http::Method,
    pub uri: http::Uri,
    pub version: http::Version,
    pub headers: http::HeaderMap,
    pub body: Vec<u8>,
}

pub(crate) struct Provider {
    pub endpoint: String,
    pub root: rustls::pki_types::CertificateDer<'static>,
    pub requests: Arc<Mutex<Vec<Observed>>>,
    replies: Arc<Mutex<VecDeque<Reply>>>,
    task: JoinHandle<()>,
}

impl Provider {
    /// Repeat the last scripted reply so retries have a stable answer.
    pub async fn start(protocol: Protocol, replies: Vec<Reply>) -> TestResult<Self> {
        assert!(!replies.is_empty());
        xmtp_cryptography::install_crypto_provider();
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
        let root = cert.der().clone();
        let mut tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![root.clone()],
                rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
            )?;
        tls.alpn_protocols = vec![b"h2".to_vec()];
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let endpoint = match protocol {
            Protocol::Http1 => format!("http://{address}"),
            Protocol::Http2Tls => format!("https://localhost:{}", address.port()),
        };
        let requests = Arc::new(Mutex::new(Vec::new()));
        let script = Arc::new(Mutex::new(VecDeque::from(replies)));
        let observed = requests.clone();
        let replies = script.clone();
        let task = tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((socket, _)) = accepted else { break; };
                        let requests = observed.clone();
                        let replies = replies.clone();
                        let acceptor = acceptor.clone();
                        tasks.spawn(async move {
                            let service = service_fn(move |request| respond(request, requests.clone(), replies.clone()));
                            match protocol {
                                Protocol::Http1 => { let _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(socket), service).await; }
                                Protocol::Http2Tls => {
                                    let Ok(socket) = acceptor.accept(socket).await else { return; };
                                    let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new()).serve_connection(TokioIo::new(socket), service).await;
                                }
                            }
                        });
                    },
                    _ = tasks.join_next(), if !tasks.is_empty() => {},
                }
            }
        });
        Ok(Self {
            endpoint,
            root,
            requests,
            replies: script,
            task,
        })
    }

    pub fn set_replies(&self, replies: Vec<Reply>) {
        assert!(!replies.is_empty());
        *self.replies.lock() = replies.into();
    }

    pub fn count(&self) -> usize {
        self.requests.lock().len()
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn respond(
    request: Request<Incoming>,
    observed: Arc<Mutex<Vec<Observed>>>,
    replies: Arc<Mutex<VecDeque<Reply>>>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, hyper::Error> {
    let (parts, body) = request.into_parts();
    let body = body.collect().await?.to_bytes().to_vec();
    observed.lock().push(Observed {
        method: parts.method,
        uri: parts.uri,
        version: parts.version,
        headers: parts.headers,
        body,
    });
    let reply = {
        let mut replies = replies.lock();
        if replies.len() > 1 {
            replies.pop_front().unwrap()
        } else {
            replies.front().unwrap().clone()
        }
    };
    if reply.stall_headers {
        std::future::pending::<()>().await;
    }
    xmtp_common::time::sleep(reply.delay).await;
    let mut response = Response::builder()
        .status(reply.status)
        .header("content-type", "application/json");
    for (key, value) in reply.headers {
        response = response.header(key, value);
    }
    let body = if reply.stall_body {
        let frames = stream::once(async { Ok(Frame::data(Bytes::from_static(b"{"))) })
            .chain(stream::pending());
        BodyExt::boxed(StreamBody::new(frames))
    } else {
        Full::new(Bytes::from(reply.body)).boxed()
    };
    Ok(response.body(body).unwrap())
}
