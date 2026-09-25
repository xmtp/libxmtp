//! PUT transport with progress measured at the socket.

use std::{
    io::{self, IoSlice},
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};

use futures_util::Stream;
use http_body_util::{BodyExt, StreamBody};
use hyper::{
    Request,
    body::{Bytes, Frame},
    client::conn::http1,
    header::{CONTENT_LENGTH, HOST, HeaderMap, HeaderName, HeaderValue},
};
use hyper_util::rt::TokioIo;
use reqwest::{Url, dns::Name};
use rustls::pki_types::ServerName;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpSocket, TcpStream},
    sync::Notify,
    time::{Instant, timeout},
};
use tokio_util::io::ReaderStream;

use super::{AbortOnDrop, Transfer};
use crate::{
    AttachmentError, AttachmentFailureCause as Cause,
    http::{PutOutcome, UploadRequest, put_outcome, secure_upload_url},
    store::{CHUNK_SIZE, StagedFile},
};

fn network() -> AttachmentError {
    AttachmentError::new(Cause::Network)
}

fn tls_config() -> Result<rustls::ClientConfig, AttachmentError> {
    #[cfg(target_os = "android")]
    {
        Ok(rustls::ClientConfig::builder()
            .with_root_certificates(rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            })
            .with_no_client_auth())
    }
    #[cfg(not(target_os = "android"))]
    {
        use rustls_platform_verifier::ConfigVerifierExt;
        rustls::ClientConfig::with_platform_verifier().map_err(|_| network())
    }
}

trait SocketIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> SocketIo for T {}

#[derive(Default)]
struct BodyWrite {
    complete: AtomicBool,
    read_failed: AtomicBool,
    flushed: AtomicBool,
    notify: Notify,
}

struct TrackedBody {
    reader: ReaderStream<tokio::fs::File>,
    write: Arc<BodyWrite>,
    remaining: u64,
}

impl Stream for TrackedBody {
    type Item = io::Result<Frame<Bytes>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.reader).poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                let Some(remaining) = self.remaining.checked_sub(bytes.len() as u64) else {
                    self.write.read_failed.store(true, Ordering::Release);
                    return Poll::Ready(Some(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "staged file changed size",
                    ))));
                };
                self.remaining = remaining;
                if remaining == 0 {
                    self.write.complete.store(true, Ordering::Release);
                }
                Poll::Ready(Some(Ok(Frame::data(bytes))))
            }
            Poll::Ready(Some(Err(error))) => {
                self.write.read_failed.store(true, Ordering::Release);
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Record bytes accepted by the socket, including encrypted TLS records.
struct ProgressIo<T> {
    inner: T,
    last: Arc<Mutex<Instant>>,
    body_write: Arc<BodyWrite>,
}

impl<T: AsyncRead + Unpin> AsyncRead for ProgressIo<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if matches!(result, Poll::Ready(Ok(()))) && buf.filled().len() > before {
            *self.last.lock().unwrap() = Instant::now();
        }
        result
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for ProgressIo<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write(cx, buf);
        if matches!(result, Poll::Ready(Ok(n)) if n > 0) {
            *self.last.lock().unwrap() = Instant::now();
        }
        result
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write_vectored(cx, bufs);
        if matches!(result, Poll::Ready(Ok(n)) if n > 0) {
            *self.last.lock().unwrap() = Instant::now();
        }
        result
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let result = Pin::new(&mut self.inner).poll_flush(cx);
        if matches!(result, Poll::Ready(Ok(())))
            && self.body_write.complete.load(Ordering::Acquire)
            && !self.body_write.read_failed.load(Ordering::Acquire)
            && !self.body_write.flushed.swap(true, Ordering::AcqRel)
        {
            self.body_write.notify.notify_one();
        }
        result
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

async fn connect(
    transfer: &Transfer,
    url: &Url,
    last: Arc<Mutex<Instant>>,
    body_write: Arc<BodyWrite>,
) -> Result<Box<dyn SocketIo>, AttachmentError> {
    let port = url.port_or_known_default().ok_or_else(network)?;
    let addresses: Vec<SocketAddr> = match url.host().ok_or_else(network)? {
        url::Host::Ipv4(ip) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
        url::Host::Ipv6(ip) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
        url::Host::Domain(host) => {
            // The backend supplies the PUT URL. Download address limits do not apply.
            let name: Name = host.parse().map_err(|_| network())?;
            let addresses = transfer
                .upload_resolver
                .resolve(name)
                .await
                .map_err(|_| network())?;
            addresses
                .map(|addr| SocketAddr::new(addr.ip(), port))
                .collect()
        }
    };
    let mut connected = None;
    for address in addresses {
        let socket = if address.is_ipv4() {
            TcpSocket::new_v4()
        } else {
            TcpSocket::new_v6()
        }
        .map_err(|_| network())?;
        socket
            .set_send_buffer_size(CHUNK_SIZE as u32)
            .map_err(|_| network())?;
        if let Ok(stream) = socket.connect(address).await {
            connected = Some(stream);
            break;
        }
    }
    let stream: TcpStream = connected.ok_or_else(network)?;
    let stream = ProgressIo {
        inner: stream,
        last,
        body_write,
    };
    if url.scheme() == "http" {
        return Ok(Box::new(stream));
    }
    xmtp_cryptography::install_crypto_provider();
    let mut config = {
        #[cfg(test)]
        {
            if let Some(roots) = &transfer.upload_test_roots {
                rustls::ClientConfig::builder()
                    .with_root_certificates(roots.clone())
                    .with_no_client_auth()
            } else {
                tls_config()?
            }
        }
        #[cfg(not(test))]
        {
            tls_config()?
        }
    };
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let server_name = match url.host().ok_or_else(network)? {
        url::Host::Domain(host) => ServerName::try_from(host.to_owned()).map_err(|_| network())?,
        url::Host::Ipv4(ip) => ServerName::IpAddress(IpAddr::V4(ip).into()),
        url::Host::Ipv6(ip) => ServerName::IpAddress(IpAddr::V6(ip).into()),
    };
    let tls = tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(server_name, stream)
        .await
        .map_err(|_| network())?;
    Ok(Box::new(tls))
}

fn request_headers(
    url: &Url,
    upload: &UploadRequest,
    body_len: u64,
) -> Result<(String, HeaderMap), AttachmentError> {
    let mut path = url.path().to_owned();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    let host = match url.host().ok_or_else(network)? {
        url::Host::Domain(host) => host.to_owned(),
        url::Host::Ipv4(ip) => ip.to_string(),
        url::Host::Ipv6(ip) => format!("[{ip}]"),
    };
    let default_port = if url.scheme() == "https" { 443 } else { 80 };
    let authority = match url.port() {
        Some(port) if port != default_port => format!("{host}:{port}"),
        _ => host,
    };
    let mut headers = HeaderMap::new();
    let mut header_bytes = path
        .len()
        .saturating_add(authority.len())
        .saturating_add(64);
    let mut has_host = false;
    let mut has_length = false;
    for (name, value) in &upload.headers {
        header_bytes = header_bytes.saturating_add(name.len() + value.len() + 4);
        if header_bytes > CHUNK_SIZE {
            return Err(AttachmentError::new(Cause::Malformed));
        }
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| AttachmentError::new(Cause::Malformed))?;
        let value =
            HeaderValue::from_str(value).map_err(|_| AttachmentError::new(Cause::Malformed))?;
        if name == CONTENT_LENGTH {
            if value
                .to_str()
                .ok()
                .and_then(|text| text.parse::<u64>().ok())
                != Some(body_len)
            {
                return Err(AttachmentError::new(Cause::Malformed));
            }
            has_length = true;
        }
        if name == HOST {
            has_host = true;
        }
        headers.append(name, value);
    }
    if !has_host {
        headers.insert(
            HOST,
            HeaderValue::from_str(&authority)
                .map_err(|_| AttachmentError::new(Cause::Malformed))?,
        );
    }
    if !has_length {
        headers.insert(
            CONTENT_LENGTH,
            HeaderValue::from_str(&body_len.to_string())
                .map_err(|_| AttachmentError::new(Cause::Malformed))?,
        );
    }
    Ok((path, headers))
}

pub(super) async fn put(
    transfer: &Transfer,
    upload: &UploadRequest,
    body: StagedFile,
) -> Result<PutOutcome, AttachmentError> {
    if upload.method != "PUT" {
        return Err(AttachmentError::new(Cause::TargetRejected));
    }
    let url = Url::parse(&upload.url).map_err(|_| AttachmentError::new(Cause::InsecureUrl))?;
    secure_upload_url(&url)?;
    let file = tokio::fs::File::open(body.path)
        .await
        .map_err(|_| AttachmentError::new(Cause::StagedUnusable))?;
    let body_len = file
        .metadata()
        .await
        .map_err(|_| AttachmentError::new(Cause::StagedUnusable))?
        .len();
    let (path, headers) = request_headers(&url, upload, body_len)?;
    let last = Arc::new(Mutex::new(Instant::now()));
    let body_write = Arc::new(BodyWrite::default());
    if body_len == 0 {
        body_write.complete.store(true, Ordering::Release);
    }
    let io = timeout(
        transfer.connect_timeout,
        connect(transfer, &url, last.clone(), body_write.clone()),
    )
    .await
    .map_err(|_| network())??;
    *last.lock().unwrap() = Instant::now();
    let stream = TrackedBody {
        reader: ReaderStream::with_capacity(file, CHUNK_SIZE),
        write: body_write.clone(),
        remaining: body_len,
    };
    let request = Request::builder()
        .method("PUT")
        .uri(path)
        .body(StreamBody::new(stream))
        .map_err(|_| AttachmentError::new(Cause::Malformed))?;
    let (mut parts, body) = request.into_parts();
    parts.headers = headers;
    let request = Request::from_parts(parts, body);
    let (mut sender, connection) = http1::handshake(TokioIo::new(io))
        .await
        .map_err(|_| network())?;
    let mut driver = tokio::spawn(connection);
    let _driver_guard = AbortOnDrop::new(&driver);
    let outcome = {
        let upload = async {
            let mut response = sender.send_request(request).await.map_err(|_| network())?;
            let outcome = put_outcome(response.status().as_u16())?;
            if outcome == PutOutcome::Stored {
                while let Some(frame) = response.frame().await {
                    frame.map_err(|_| network())?;
                }
            }
            Ok(outcome)
        };
        tokio::pin!(upload);
        loop {
            let deadline = *last.lock().unwrap() + transfer.idle_timeout;
            tokio::select! {
                result = &mut upload => break result,
                () = tokio::time::sleep_until(deadline) => {
                    if Instant::now().duration_since(*last.lock().unwrap()) >= transfer.idle_timeout {
                        break Err(network());
                    }
                }
            }
        }
    };
    drop(sender);
    let outcome = if matches!(outcome, Ok(PutOutcome::Stored)) {
        loop {
            if body_write.flushed.load(Ordering::Acquire) {
                break Ok(PutOutcome::Stored);
            }
            let deadline = *last.lock().unwrap() + transfer.idle_timeout;
            tokio::select! {
                () = body_write.notify.notified() => {}
                result = &mut driver => {
                    break if result.is_ok_and(|result| result.is_ok())
                        && body_write.flushed.load(Ordering::Acquire)
                    {
                        Ok(PutOutcome::Stored)
                    } else {
                        Err(network())
                    };
                }
                () = tokio::time::sleep_until(deadline) => {
                    if Instant::now().duration_since(*last.lock().unwrap()) >= transfer.idle_timeout {
                        break Err(network());
                    }
                }
            }
        }
    } else {
        outcome
    };
    driver.abort();
    outcome
}
