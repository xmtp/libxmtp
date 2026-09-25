//! PUT transport with progress measured at the socket.

use std::{
    io::{self, IoSlice},
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use reqwest::{
    Url,
    dns::{Name, Resolve},
    header::{CONTENT_LENGTH, HOST, HeaderName, HeaderValue},
};
use rustls::pki_types::ServerName;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, ReadBuf},
    net::{TcpSocket, TcpStream},
    time::{Instant, timeout},
};

use super::{BlockedDns, Transfer, validate_url};
use crate::{
    AttachmentError, AttachmentFailureCause as Cause,
    http::{PutOutcome, UploadRequest, put_outcome, sensitive_header},
    store::{CHUNK_SIZE, StagedFile},
};

fn network() -> AttachmentError {
    AttachmentError::new(Cause::Network)
}

trait SocketIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> SocketIo for T {}

/// Record bytes accepted by the socket, including encrypted TLS records.
struct ProgressIo<T> {
    inner: T,
    last: Arc<Mutex<Instant>>,
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
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

async fn connect(
    transfer: &Transfer,
    url: &Url,
    last: Arc<Mutex<Instant>>,
) -> Result<Box<dyn SocketIo>, AttachmentError> {
    let port = url.port_or_known_default().ok_or_else(network)?;
    let addresses: Vec<SocketAddr> = match url.host().ok_or_else(network)? {
        url::Host::Ipv4(ip) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
        url::Host::Ipv6(ip) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
        url::Host::Domain(host) => {
            let name: Name = host.parse().map_err(|_| network())?;
            let addresses = transfer.resolver.resolve(name).await.map_err(|error| {
                if error.is::<BlockedDns>() {
                    AttachmentError::new(Cause::BlockedAddress)
                } else {
                    network()
                }
            })?;
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
    };
    if url.scheme() == "http" {
        return Ok(Box::new(stream));
    }
    xmtp_cryptography::install_crypto_provider();
    #[cfg(target_os = "android")]
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        })
        .with_no_client_auth();
    #[cfg(not(target_os = "android"))]
    let config = {
        use rustls_platform_verifier::ConfigVerifierExt;
        rustls::ClientConfig::with_platform_verifier().map_err(|_| network())?
    };
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
) -> Result<Vec<u8>, AttachmentError> {
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
    let mut headers = format!("PUT {path} HTTP/1.1\r\n").into_bytes();
    let mut has_host = false;
    let mut has_length = false;
    for (name, value) in &upload.headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| AttachmentError::new(Cause::Malformed))?;
        if sensitive_header(name.as_str()) {
            return Err(AttachmentError::new(Cause::Credential));
        }
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
        headers.extend_from_slice(name.as_str().as_bytes());
        headers.extend_from_slice(b": ");
        headers.extend_from_slice(value.as_bytes());
        headers.extend_from_slice(b"\r\n");
        if headers.len() > CHUNK_SIZE {
            return Err(AttachmentError::new(Cause::Malformed));
        }
    }
    if !has_host {
        headers.extend_from_slice(format!("host: {authority}\r\n").as_bytes());
    }
    if !has_length {
        headers.extend_from_slice(format!("content-length: {body_len}\r\n").as_bytes());
    }
    headers.extend_from_slice(b"\r\n");
    Ok(headers)
}

async fn status(io: Box<dyn SocketIo>) -> Result<u16, AttachmentError> {
    let mut io = BufReader::with_capacity(4096, io);
    let mut response = [0_u8; 16 * 1024];
    let mut used = 0;
    loop {
        io.read_exact(&mut response[used..used + 1])
            .await
            .map_err(|_| network())?;
        used += 1;
        if used >= 4 && &response[used - 4..used] == b"\r\n\r\n" {
            let first = response[..used]
                .windows(2)
                .position(|bytes| bytes == b"\r\n")
                .ok_or_else(network)?;
            let line = std::str::from_utf8(&response[..first]).map_err(|_| network())?;
            let mut parts = line.split_ascii_whitespace();
            if !parts
                .next()
                .is_some_and(|version| version.starts_with("HTTP/1."))
            {
                return Err(network());
            }
            let code = parts
                .next()
                .and_then(|code| code.parse::<u16>().ok())
                .ok_or_else(network)?;
            if (100..200).contains(&code) {
                used = 0;
                continue;
            }
            return Ok(code);
        }
        if used == response.len() {
            return Err(network());
        }
    }
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
    validate_url(&url, &transfer.options)?;
    let mut file = tokio::fs::File::open(body.path)
        .await
        .map_err(|_| AttachmentError::new(Cause::StagedUnusable))?;
    let body_len = file
        .metadata()
        .await
        .map_err(|_| AttachmentError::new(Cause::StagedUnusable))?
        .len();
    let headers = request_headers(&url, upload, body_len)?;
    let last = Arc::new(Mutex::new(Instant::now()));
    let mut io = timeout(
        transfer.connect_timeout,
        connect(transfer, &url, last.clone()),
    )
    .await
    .map_err(|_| network())??;
    *last.lock().unwrap() = Instant::now();
    let upload = async {
        io.write_all(&headers).await.map_err(|_| network())?;
        let mut buffer = [0_u8; CHUNK_SIZE];
        loop {
            let size = file
                .read(&mut buffer)
                .await
                .map_err(|_| AttachmentError::new(Cause::StagedUnusable))?;
            if size == 0 {
                break;
            }
            io.write_all(&buffer[..size]).await.map_err(|_| network())?;
        }
        io.flush().await.map_err(|_| network())?;
        status(io).await
    };
    tokio::pin!(upload);
    loop {
        let deadline = *last.lock().unwrap() + transfer.idle_timeout;
        tokio::select! {
            result = &mut upload => return put_outcome(result?),
            () = tokio::time::sleep_until(deadline) => {
                if Instant::now().duration_since(*last.lock().unwrap()) >= transfer.idle_timeout {
                    return Err(network());
                }
            }
        }
    }
}
