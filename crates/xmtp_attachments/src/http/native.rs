use std::{
    error::Error,
    io::Read,
    net::{IpAddr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use reqwest::{
    Client, StatusCode, Url,
    dns::{Addrs, Name, Resolve, Resolving},
    header::{ACCEPT_ENCODING, CONTENT_ENCODING, LOCATION},
    redirect::Policy,
};
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::{CONNECT_TIMEOUT, IDLE_TIMEOUT, PutOutcome, UploadRequest, checked_count};
use crate::{
    AttachmentError, AttachmentFailureCause as Cause,
    address::is_private,
    store::{AttachmentOptions, CHUNK_SIZE, DownloadSink, StagedFile},
};

mod upload;

#[derive(Debug)]
struct BlockedDns;

impl std::fmt::Display for BlockedDns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("all DNS answers are blocked attachment addresses")
    }
}

impl Error for BlockedDns {}

struct SystemResolver;

impl Resolve for SystemResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addrs: Vec<_> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

struct GuardedResolver {
    upstream: Arc<dyn Resolve>,
    allow_private: bool,
}

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let answer = self.upstream.resolve(name);
        let allow_private = self.allow_private;
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = answer
                .await?
                .filter(|addr| allow_private || !is_private(addr.ip()))
                .collect();
            if addrs.is_empty() {
                return Err(Box::new(BlockedDns) as Box<dyn Error + Send + Sync>);
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

fn reqwest_error(error: reqwest::Error) -> AttachmentError {
    let mut source: Option<&(dyn Error + 'static)> = Some(&error);
    while let Some(current) = source {
        if current.is::<BlockedDns>() {
            return AttachmentError::new(Cause::BlockedAddress);
        }
        source = current.source();
    }
    AttachmentError::new(Cause::Network)
}

fn validate_url(url: &Url, options: &AttachmentOptions) -> Result<(), AttachmentError> {
    let host = url.host().ok_or(AttachmentError::new(Cause::InsecureUrl))?;
    let loopback = match host {
        url::Host::Domain(name) => name.eq_ignore_ascii_case("localhost"),
        url::Host::Ipv4(address) => address.is_loopback(),
        url::Host::Ipv6(address) => address.is_loopback(),
    };
    if url.scheme() != "https"
        && !(url.scheme() == "http" && loopback && options.allow_private_network)
    {
        return Err(AttachmentError::new(Cause::InsecureUrl));
    }
    let ip = match host {
        url::Host::Ipv4(ip) => Some(IpAddr::V4(ip)),
        url::Host::Ipv6(ip) => Some(IpAddr::V6(ip)),
        url::Host::Domain(_) => None,
    };
    if ip.is_some_and(|address| !options.allow_private_network && is_private(address)) {
        return Err(AttachmentError::new(Cause::BlockedAddress));
    }
    Ok(())
}

fn validate_upload_url(url: &Url) -> Result<(), AttachmentError> {
    let host = url.host().ok_or(AttachmentError::new(Cause::InsecureUrl))?;
    let loopback = match host {
        url::Host::Domain(name) => name.eq_ignore_ascii_case("localhost"),
        url::Host::Ipv4(address) => address.is_loopback(),
        url::Host::Ipv6(address) => address.is_loopback(),
    };
    if url.scheme() == "https" || (url.scheme() == "http" && loopback) {
        Ok(())
    } else {
        Err(AttachmentError::new(Cause::InsecureUrl))
    }
}

fn redirect_target(
    current: &Url,
    location: &str,
    redirects: u8,
    options: &AttachmentOptions,
) -> Result<Url, AttachmentError> {
    if redirects == 10 {
        return Err(AttachmentError::new(Cause::TooManyRedirects));
    }
    let target = current
        .join(location)
        .map_err(|_| AttachmentError::new(Cause::InsecureUrl))?;
    validate_url(&target, options)?;
    Ok(target)
}

/// Native HTTP transfer. Downloads check each connected address.
pub struct Transfer {
    client: Client,
    upload_resolver: Arc<dyn Resolve>,
    options: AttachmentOptions,
    connect_timeout: Duration,
    idle_timeout: Duration,
}

impl Transfer {
    pub fn new(options: AttachmentOptions) -> Result<Self, AttachmentError> {
        Self::with_resolver(options, Arc::new(SystemResolver))
    }

    fn with_resolver(
        options: AttachmentOptions,
        upstream: Arc<dyn Resolve>,
    ) -> Result<Self, AttachmentError> {
        Self::with_resolver_and_timeouts(options, upstream, CONNECT_TIMEOUT, IDLE_TIMEOUT)
    }

    fn with_resolver_and_timeouts(
        options: AttachmentOptions,
        upstream: Arc<dyn Resolve>,
        connect_timeout: Duration,
        idle_timeout: Duration,
    ) -> Result<Self, AttachmentError> {
        let resolver = Arc::new(GuardedResolver {
            upstream: upstream.clone(),
            allow_private: options.allow_private_network,
        });
        let client = xmtp_common::http::client_builder()
            .no_proxy()
            .dns_resolver(resolver.clone())
            .redirect(Policy::none())
            .connect_timeout(connect_timeout)
            .build()
            .map_err(reqwest_error)?;
        Ok(Self {
            client,
            upload_resolver: upstream,
            options,
            connect_timeout,
            idle_timeout,
        })
    }

    #[cfg(test)]
    fn with_timeouts(
        options: AttachmentOptions,
        connect: Duration,
        idle: Duration,
    ) -> Result<Self, AttachmentError> {
        Self::with_resolver_and_timeouts(options, Arc::new(SystemResolver), connect, idle)
    }

    pub async fn put(
        &self,
        request: &UploadRequest,
        body: StagedFile,
    ) -> Result<PutOutcome, AttachmentError> {
        upload::put(self, request, body).await
    }

    pub async fn get(
        &self,
        url: &str,
        cap: u64,
        sink: &mut dyn DownloadSink,
    ) -> Result<(), AttachmentError> {
        let mut url = Url::parse(url).map_err(|_| AttachmentError::new(Cause::InsecureUrl))?;
        let mut redirects = 0;
        let response = loop {
            validate_url(&url, &self.options)?;
            let response = timeout(
                self.idle_timeout,
                self.client
                    .get(url.clone())
                    .header(ACCEPT_ENCODING, "identity")
                    .send(),
            )
            .await
            .map_err(|_| AttachmentError::new(Cause::Network))?
            .map_err(reqwest_error)?;
            if !response.status().is_redirection() {
                break response;
            }
            let location = response
                .headers()
                .get(LOCATION)
                .ok_or(AttachmentError::new(Cause::HttpStatus))?
                .to_str()
                .map_err(|_| AttachmentError::new(Cause::HttpStatus))?;
            url = redirect_target(&url, location, redirects, &self.options)?;
            redirects += 1;
        };
        match response.status() {
            StatusCode::OK => {}
            StatusCode::NOT_FOUND | StatusCode::GONE => {
                return Err(AttachmentError::new(Cause::NotFound));
            }
            _ => return Err(AttachmentError::new(Cause::HttpStatus)),
        }
        let encoding = response
            .headers()
            .get(CONTENT_ENCODING)
            .map(|value| value.to_str().unwrap_or("invalid").to_ascii_lowercase());
        let cap = cap.min(self.options.max_download_bytes.unwrap_or(u64::MAX));
        match encoding.as_deref() {
            None | Some("identity") => read_identity(response, cap, sink, self.idle_timeout).await,
            Some("gzip" | "deflate") => {
                read_compressed(
                    response,
                    encoding.as_deref().unwrap(),
                    cap,
                    sink,
                    self.idle_timeout,
                )
                .await
            }
            _ => Err(AttachmentError::new(Cause::HttpStatus)),
        }
    }
}

async fn read_identity(
    mut response: reqwest::Response,
    cap: u64,
    sink: &mut dyn DownloadSink,
    idle_timeout: Duration,
) -> Result<(), AttachmentError> {
    let mut count = 0;
    while let Some(chunk) = timeout(idle_timeout, response.chunk())
        .await
        .map_err(|_| AttachmentError::new(Cause::Network))?
        .map_err(reqwest_error)?
    {
        count = checked_count(count, chunk.len(), cap)?;
        sink.write(&chunk).await?;
    }
    Ok(())
}

struct ChannelReader {
    rx: mpsc::Receiver<Bytes>,
    consumed: mpsc::Sender<()>,
    chunk: Bytes,
    position: usize,
}

impl Read for ChannelReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.position == self.chunk.len() {
            if !self.chunk.is_empty() {
                self.chunk = Bytes::new();
                if self.consumed.blocking_send(()).is_err() {
                    return Ok(0);
                }
            }
            let Some(bytes) = self.rx.blocking_recv() else {
                return Ok(0);
            };
            self.chunk = bytes;
            self.position = 0;
        }
        let count = output.len().min(self.chunk.len() - self.position);
        output[..count].copy_from_slice(&self.chunk[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

async fn read_compressed(
    mut response: reqwest::Response,
    encoding: &str,
    cap: u64,
    sink: &mut dyn DownloadSink,
    idle_timeout: Duration,
) -> Result<(), AttachmentError> {
    let (input_tx, input_rx) = mpsc::channel(1);
    let (consumed_tx, mut consumed_rx) = mpsc::channel(1);
    let (output_tx, mut output_rx) = mpsc::channel(1);
    let (recycle_tx, mut recycle_rx) = mpsc::channel(1);
    let network_failed = Arc::new(AtomicBool::new(false));
    let producer_failed = network_failed.clone();
    let gzip = encoding == "gzip";
    let producer = tokio::spawn(async move {
        loop {
            let chunk = match timeout(idle_timeout, response.chunk()).await {
                Ok(Ok(Some(chunk))) => chunk,
                Ok(Ok(None)) => break,
                Err(_) | Ok(Err(_)) => {
                    producer_failed.store(true, Ordering::Release);
                    break;
                }
            };
            for start in (0..chunk.len()).step_by(CHUNK_SIZE) {
                let end = (start + CHUNK_SIZE).min(chunk.len());
                if input_tx.send(chunk.slice(start..end)).await.is_err() {
                    return;
                }
                if consumed_rx.recv().await.is_none() {
                    return;
                }
            }
        }
    });
    let decoder = tokio::task::spawn_blocking(move || {
        let reader = ChannelReader {
            rx: input_rx,
            consumed: consumed_tx,
            chunk: Bytes::new(),
            position: 0,
        };
        let mut reader: Box<dyn Read> = if gzip {
            Box::new(flate2::read::GzDecoder::new(reader))
        } else {
            Box::new(flate2::read::ZlibDecoder::new(reader))
        };
        let mut output = vec![0_u8; CHUNK_SIZE];
        loop {
            let count = reader.read(&mut output).map_err(|_| ())?;
            if count == 0 {
                return Ok::<_, ()>(());
            }
            if output_tx.blocking_send((output, count)).is_err() {
                return Ok(());
            }
            let Some(recycled) = recycle_rx.blocking_recv() else {
                return Ok(());
            };
            output = recycled;
        }
    });
    let mut count = 0;
    let mut result = Ok(());
    while let Some((output, size)) = output_rx.recv().await {
        match checked_count(count, size, cap) {
            Ok(next) => count = next,
            Err(error) => {
                result = Err(error);
                break;
            }
        }
        if let Err(error) = sink.write(&output[..size]).await {
            result = Err(error);
            break;
        }
        if recycle_tx.send(output).await.is_err() {
            break;
        }
    }
    drop(recycle_tx);
    drop(output_rx);
    producer.abort();
    if result.is_ok() {
        result = decoder
            .await
            .map_err(|_| AttachmentError::new(Cause::HttpStatus))?
            .map_err(|_| {
                AttachmentError::new(if network_failed.load(Ordering::Acquire) {
                    Cause::Network
                } else {
                    Cause::HttpStatus
                })
            });
    } else {
        let _ = decoder.await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{LocalStore, NativeStore};
    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::{Request, Response, body::Incoming, server::conn::http1, service::service_fn};
    use hyper_util::rt::TokioIo;
    use reqwest::Method;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::time::Instant;

    struct Server {
        url: String,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn server<F>(handler: F) -> Server
    where
        F: Fn(Request<Incoming>) -> Response<Full<Bytes>> + Send + Sync + 'static,
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handler = Arc::new(handler);
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let handler = handler.clone();
                tokio::spawn(async move {
                    let service = service_fn(move |request| {
                        let response = handler(request);
                        async move { Ok::<_, std::convert::Infallible>(response) }
                    });
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        Server {
            url: format!("http://127.0.0.1:{port}"),
            task,
        }
    }

    fn answer(status: StatusCode, body: impl Into<Bytes>) -> Response<Full<Bytes>> {
        Response::builder()
            .status(status)
            .body(Full::new(body.into()))
            .unwrap()
    }

    fn allowed() -> Transfer {
        Transfer::new(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .unwrap()
    }

    #[derive(Default)]
    struct MemorySink(Vec<u8>);

    #[async_trait::async_trait]
    impl DownloadSink for MemorySink {
        async fn write(&mut self, bytes: &[u8]) -> Result<(), AttachmentError> {
            self.0.extend_from_slice(bytes);
            Ok(())
        }
    }

    // verifies: ATCH-053
    #[xmtp_common::test(unwrap_try = true)]
    async fn rejects_http_public() {
        let error = allowed()
            .get("http://example.com/object", 100, &mut MemorySink::default())
            .await
            .unwrap_err();
        assert_eq!(error.cause, Cause::InsecureUrl);
    }

    // verifies: ATCH-053
    #[xmtp_common::test(unwrap_try = true)]
    async fn allows_http_loopback_when_permitted() {
        let server = server(|_| answer(StatusCode::OK, "hello")).await;
        let mut sink = MemorySink::default();
        allowed().get(&server.url, 5, &mut sink).await?;
        assert_eq!(sink.0, b"hello");
    }

    struct FakeResolver(Vec<SocketAddr>);

    struct PendingResolver;

    impl Resolve for PendingResolver {
        fn resolve(&self, _name: Name) -> Resolving {
            Box::pin(std::future::pending())
        }
    }

    impl Resolve for FakeResolver {
        fn resolve(&self, _name: Name) -> Resolving {
            let addresses = self.0.clone();
            Box::pin(async move { Ok(Box::new(addresses.into_iter()) as Addrs) })
        }
    }

    fn staged_body() -> std::io::Result<(tempfile::TempDir, StagedFile)> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("body");
        std::fs::write(&path, b"body")?;
        Ok((directory, StagedFile { path }))
    }

    fn upload(url: String) -> UploadRequest {
        UploadRequest {
            method: "PUT".into(),
            url,
            headers: vec![],
            expires_in_seconds: 60,
        }
    }

    // verifies: ATCH-071
    #[xmtp_common::test(unwrap_try = true)]
    async fn public_http_put_is_rejected_before_request() {
        let seen = Arc::new(AtomicBool::new(false));
        let flag = seen.clone();
        let server = server(move |_| {
            flag.store(true, Ordering::Relaxed);
            answer(StatusCode::OK, "")
        })
        .await;
        let address: SocketAddr = server.url.trim_start_matches("http://").parse()?;
        let transfer = Transfer::with_resolver(
            AttachmentOptions::default(),
            Arc::new(FakeResolver(vec![address])),
        )?;
        let (_directory, body) = staged_body()?;
        let request = upload(format!("http://example.com:{}/object", address.port()));
        assert_eq!(
            transfer.put(&request, body).await.unwrap_err().cause,
            Cause::InsecureUrl
        );
        assert!(!seen.load(Ordering::Relaxed));
    }

    // verifies: ATCH-071
    #[xmtp_common::test(unwrap_try = true)]
    async fn loopback_http_put_needs_no_private_network_flag() {
        let server = server(|_| answer(StatusCode::ACCEPTED, "")).await;
        let (_directory, body) = staged_body()?;
        let transfer = Transfer::new(AttachmentOptions::default())?;
        assert_eq!(
            transfer.put(&upload(server.url.clone()), body).await?,
            PutOutcome::Stored
        );
    }

    // verifies: ATCH-071
    #[xmtp_common::test(unwrap_try = true)]
    async fn private_https_put_is_not_dns_blocked() {
        validate_upload_url(&Url::parse("https://10.1.2.3/object")?)?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let transfer = Transfer::with_resolver_and_timeouts(
            AttachmentOptions::default(),
            Arc::new(FakeResolver(vec![address])),
            Duration::from_secs(1),
            Duration::from_secs(1),
        )?;
        let (_directory, body) = staged_body()?;
        let accepted = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            drop(socket);
        });
        let request = upload(format!("https://private.example:{}/object", address.port()));
        assert_eq!(
            transfer.put(&request, body).await.unwrap_err().cause,
            Cause::Network
        );
        tokio::time::timeout(Duration::from_secs(1), accepted).await??;
    }

    // verifies: ATCH-071
    #[xmtp_common::test(unwrap_try = true)]
    async fn put_redirect_is_rejected_without_following() {
        let followed = Arc::new(AtomicBool::new(false));
        let flag = followed.clone();
        let target = server(move |_| {
            flag.store(true, Ordering::Relaxed);
            answer(StatusCode::OK, "")
        })
        .await;
        let location = target.url.clone();
        let origin = server(move |_| {
            Response::builder()
                .status(StatusCode::TEMPORARY_REDIRECT)
                .header(LOCATION, &location)
                .body(Full::new(Bytes::new()))
                .unwrap()
        })
        .await;
        let (_directory, body) = staged_body()?;
        let transfer = Transfer::new(AttachmentOptions::default())?;
        assert_eq!(
            transfer
                .put(&upload(origin.url.clone()), body)
                .await
                .unwrap_err()
                .cause,
            Cause::TargetRejected
        );
        assert!(!followed.load(Ordering::Relaxed));
    }

    async fn early_put_status(
        status: StatusCode,
    ) -> Result<Result<PutOutcome, AttachmentError>, Box<dyn Error + Send + Sync>> {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

        const BODY_SIZE: usize = 8 * 1024 * 1024;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(socket);
            let mut line = String::new();
            let mut content_length = None;
            loop {
                line.clear();
                assert!(reader.read_line(&mut line).await.unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(length) = line.to_ascii_lowercase().strip_prefix("content-length: ") {
                    content_length = Some(length.trim().parse::<usize>().unwrap());
                }
            }
            assert_eq!(content_length, Some(BODY_SIZE));
            reader
                .get_mut()
                .write_all(
                    format!(
                        "HTTP/1.1 {} {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        status.as_u16(),
                        status.canonical_reason().unwrap()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            let mut discarded = 0;
            let mut buffer = [0_u8; 8192];
            let _ = tokio::time::timeout(Duration::from_millis(200), async {
                while discarded < 256 * 1024 {
                    let read = reader.read(&mut buffer).await.unwrap();
                    if read == 0 {
                        break;
                    }
                    discarded += read;
                }
            })
            .await;
            assert!(discarded <= 256 * 1024);
        });
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("body");
        std::fs::write(&path, vec![0x5a; BODY_SIZE])?;
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            allowed().put(&upload(url), StagedFile { path }),
        )
        .await?;
        tokio::time::timeout(Duration::from_secs(1), server).await??;
        Ok(result)
    }

    // verifies: ATCH-025
    #[xmtp_common::test(unwrap_try = true)]
    async fn put_early_412_with_large_body() {
        assert_eq!(
            early_put_status(StatusCode::PRECONDITION_FAILED).await??,
            PutOutcome::AlreadyStored
        );
    }

    // verifies: ATCH-025
    #[xmtp_common::test(unwrap_try = true)]
    async fn put_early_403_with_large_body() {
        assert_eq!(
            early_put_status(StatusCode::FORBIDDEN)
                .await?
                .unwrap_err()
                .cause,
            Cause::TargetRejected
        );
    }

    // verifies: ATCH-054
    #[xmtp_common::test(unwrap_try = true)]
    async fn downloads_ignore_proxy_environment() {
        let proxy = std::net::TcpListener::bind("127.0.0.1:0")?;
        let proxy_url = format!("http://{}", proxy.local_addr()?);
        let output = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "http::native::tests::proxy_environment_child"])
            .env("XMTP_ATTACHMENTS_PROXY_CHILD", "1")
            .env("HTTP_PROXY", &proxy_url)
            .env("HTTPS_PROXY", &proxy_url)
            .env("ALL_PROXY", &proxy_url)
            .env("NO_PROXY", "")
            .output()?;
        assert!(
            output.status.success(),
            "child stdout: {}\nchild stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn proxy_environment_child() {
        if std::env::var_os("XMTP_ATTACHMENTS_PROXY_CHILD").is_none() {
            return;
        }
        let target = server(|_| answer(StatusCode::OK, "direct")).await;
        let mut sink = MemorySink::default();
        allowed().get(&target.url, 6, &mut sink).await?;
        assert_eq!(sink.0, b"direct");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let transfer = Transfer::with_resolver_and_timeouts(
            AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            },
            Arc::new(FakeResolver(vec![address])),
            Duration::from_secs(1),
            Duration::from_secs(1),
        )?;
        let task = tokio::spawn(async move {
            transfer
                .get(
                    &format!("https://example.test:{}/object", address.port()),
                    1,
                    &mut MemorySink::default(),
                )
                .await
        });
        let (socket, _) = tokio::time::timeout(Duration::from_secs(1), listener.accept()).await??;
        drop(socket);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), task)
                .await??
                .unwrap_err()
                .cause,
            Cause::Network
        );
    }

    // verifies: ATCH-054
    #[xmtp_common::test(unwrap_try = true)]
    async fn blocks_private_after_dns() {
        for address in ["10.0.0.1", "::ffff:10.0.0.1", "64:ff9b::a00:1"] {
            let address: IpAddr = address.parse()?;
            let resolver = Arc::new(FakeResolver(vec![SocketAddr::new(address, 443)]));
            let guarded = GuardedResolver {
                upstream: resolver.clone(),
                allow_private: false,
            };
            let failure = guarded.resolve("example.test".parse()?).await;
            assert!(failure.err().unwrap().is::<BlockedDns>());
            let transfer = Transfer::with_resolver(AttachmentOptions::default(), resolver)?;
            let error = transfer
                .get(
                    "https://example.test/object",
                    10,
                    &mut MemorySink::default(),
                )
                .await
                .unwrap_err();
            assert_eq!(error.cause, Cause::BlockedAddress);
        }
    }

    // verifies: ATCH-054
    #[xmtp_common::test(unwrap_try = true)]
    async fn blocks_ip_literal() {
        for url in [
            "https://10.0.0.1/object",
            "https://[::ffff:10.0.0.1]/object",
        ] {
            let error = Transfer::new(AttachmentOptions::default())?
                .get(url, 10, &mut MemorySink::default())
                .await
                .unwrap_err();
            assert_eq!(error.cause, Cause::BlockedAddress);
        }
    }

    // verifies: ATCH-055
    #[xmtp_common::test(unwrap_try = true)]
    async fn redirect_rechecked() {
        let server = server(|_| {
            Response::builder()
                .status(StatusCode::FOUND)
                .header(LOCATION, "http://10.0.0.1/private")
                .body(Full::new(Bytes::new()))
                .unwrap()
        })
        .await;
        let error = allowed()
            .get(&server.url, 10, &mut MemorySink::default())
            .await
            .unwrap_err();
        // The permitted loopback source cannot make an insecure target safe.
        assert_eq!(error.cause, Cause::InsecureUrl);
        let public = Url::parse("https://public.example/object")?;
        assert_eq!(
            redirect_target(
                &public,
                "https://10.0.0.1/private",
                0,
                &AttachmentOptions::default(),
            )
            .unwrap_err()
            .cause,
            Cause::BlockedAddress
        );
    }

    // verifies: ATCH-055
    #[xmtp_common::test(unwrap_try = true)]
    async fn ten_redirects_succeed() {
        let server = server(|request| {
            let n: u8 = request
                .uri()
                .path()
                .trim_start_matches('/')
                .parse()
                .unwrap();
            if n < 10 {
                Response::builder()
                    .status(StatusCode::FOUND)
                    .header(LOCATION, format!("/{}", n + 1))
                    .body(Full::new(Bytes::new()))
                    .unwrap()
            } else {
                answer(StatusCode::OK, "done")
            }
        })
        .await;
        let mut sink = MemorySink::default();
        allowed()
            .get(&format!("{}/0", server.url), 10, &mut sink)
            .await?;
        assert_eq!(sink.0, b"done");
    }

    // verifies: ATCH-055
    #[xmtp_common::test(unwrap_try = true)]
    async fn eleven_redirects_fail() {
        let server = server(|request| {
            let n: u8 = request
                .uri()
                .path()
                .trim_start_matches('/')
                .parse()
                .unwrap_or(0);
            if n < 11 {
                Response::builder()
                    .status(StatusCode::FOUND)
                    .header(LOCATION, format!("/{}", n + 1))
                    .body(Full::new(Bytes::new()))
                    .unwrap()
            } else {
                answer(StatusCode::OK, "done")
            }
        })
        .await;
        let error = allowed()
            .get(&format!("{}/0", server.url), 10, &mut MemorySink::default())
            .await
            .unwrap_err();
        assert_eq!(error.cause, Cause::TooManyRedirects);
    }

    // verifies: ATCH-056
    #[xmtp_common::test(unwrap_try = true)]
    async fn gzip_body_capped_after_decoding() {
        use std::io::Write;
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzip.write_all(&vec![b'x'; 128 * 1024])?;
        let compressed = Bytes::from(gzip.finish()?);
        let server = server(move |_| {
            Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_ENCODING, "gzip")
                .body(Full::new(compressed.clone()))
                .unwrap()
        })
        .await;
        let error = allowed()
            .get(&server.url, 1024, &mut MemorySink::default())
            .await
            .unwrap_err();
        assert_eq!(error.cause, Cause::TooLarge);
    }

    // verifies: ATCH-056
    #[xmtp_common::test(unwrap_try = true)]
    async fn identity_body_capped() {
        let server = server(|_| answer(StatusCode::OK, "five!")).await;
        let error = allowed()
            .get(&server.url, 4, &mut MemorySink::default())
            .await
            .unwrap_err();
        assert_eq!(error.cause, Cause::TooLarge);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn gzip_network_error_is_network() {
        use std::io::Write;
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzip.write_all(&vec![b'x'; 128 * 1024])?;
        let compressed = gzip.finish()?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let task = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).await;
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 1000\r\n\r\n",
                )
                .await
                .unwrap();
            socket
                .write_all(&compressed[..compressed.len() / 2])
                .await
                .unwrap();
        });
        let error = allowed()
            .get(&url, 1024 * 1024, &mut MemorySink::default())
            .await
            .unwrap_err();
        assert_eq!(error.cause, Cause::Network);
        task.await?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn gzip_and_deflate_decode_in_stream() {
        use std::io::Write;
        let content = b"decoded ciphertext";
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzip.write_all(content)?;
        let mut deflate = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        deflate.write_all(content)?;
        for (encoding, compressed) in [("gzip", gzip.finish()?), ("deflate", deflate.finish()?)] {
            let compressed = Bytes::from(compressed);
            let server = server(move |_| {
                Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_ENCODING, encoding)
                    .body(Full::new(compressed.clone()))
                    .unwrap()
            })
            .await;
            let mut sink = MemorySink::default();
            allowed()
                .get(&server.url, content.len() as u64, &mut sink)
                .await?;
            assert_eq!(sink.0, content);
        }
    }

    // verifies: ATCH-056
    #[xmtp_common::test(unwrap_try = true)]
    async fn unknown_encoding_rejected() {
        let server = server(|_| {
            Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_ENCODING, "br")
                .body(Full::new(Bytes::from_static(b"data")))
                .unwrap()
        })
        .await;
        assert_eq!(
            allowed()
                .get(&server.url, 10, &mut MemorySink::default())
                .await
                .unwrap_err()
                .cause,
            Cause::HttpStatus
        );
    }

    // verifies: ATCH-057
    #[xmtp_common::test(unwrap_try = true)]
    async fn status_mapping() {
        for (status, expected) in [
            (StatusCode::NOT_FOUND, Cause::NotFound),
            (StatusCode::GONE, Cause::NotFound),
            (StatusCode::INTERNAL_SERVER_ERROR, Cause::HttpStatus),
        ] {
            let server = server(move |_| answer(status, "hidden")).await;
            let mut sink = MemorySink::default();
            assert_eq!(
                allowed()
                    .get(&server.url, 100, &mut sink)
                    .await
                    .unwrap_err()
                    .cause,
                expected
            );
            assert!(sink.0.is_empty());
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn no_credential_headers() {
        let seen_get = Arc::new(AtomicBool::new(false));
        let seen_put = Arc::new(AtomicBool::new(false));
        let get_flag = seen_get.clone();
        let put_flag = seen_put.clone();
        let server = server(move |request| {
            let clean = request.headers().get("authorization").is_none()
                && request.headers().get("cookie").is_none()
                && request.headers().get("x-xmtp-inbox-id").is_none();
            if request.method() == Method::PUT {
                put_flag.store(clean, Ordering::Relaxed);
                answer(StatusCode::ACCEPTED, "")
            } else {
                get_flag.store(
                    clean && request.headers().get(ACCEPT_ENCODING).unwrap() == "identity",
                    Ordering::Relaxed,
                );
                answer(StatusCode::OK, "x")
            }
        })
        .await;
        allowed()
            .get(&server.url, 10, &mut MemorySink::default())
            .await?;
        assert!(seen_get.load(Ordering::Relaxed));

        let directory = tempfile::tempdir()?;
        let store = NativeStore::new(directory.path()).await?;
        let staged = crate::store::staged_path(&"a".repeat(64))?;
        let mut writer = store.create_temp(".tmp/body").await?;
        writer.write(b"body").await?;
        store.sync(&mut writer).await?;
        drop(writer);
        store.rename(".tmp/body", &staged).await?;
        let upload = UploadRequest {
            method: "PUT".into(),
            url: server.url.clone(),
            headers: vec![],
            expires_in_seconds: 60,
        };
        assert_eq!(
            allowed()
                .put(&upload, store.open_read(&staged).await?)
                .await?,
            PutOutcome::Stored
        );
        assert!(seen_put.load(Ordering::Relaxed));
        let upload = UploadRequest {
            headers: vec![("authorization".into(), "secret".into())],
            ..upload
        };
        let error = allowed()
            .put(&upload, store.open_read(&staged).await?)
            .await
            .unwrap_err();
        assert_eq!(error.cause, Cause::Credential);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn upload_precondition_is_already_stored() {
        let server = server(|_| answer(StatusCode::PRECONDITION_FAILED, "")).await;
        let directory = tempfile::tempdir()?;
        let store = NativeStore::new(directory.path()).await?;
        let staged = crate::store::staged_path(&"a".repeat(64))?;
        let mut writer = store.create_temp(".tmp/body").await?;
        writer.write(b"body").await?;
        store.sync(&mut writer).await?;
        drop(writer);
        store.rename(".tmp/body", &staged).await?;
        let upload = UploadRequest {
            method: "PUT".into(),
            url: server.url.clone(),
            headers: vec![],
            expires_in_seconds: 60,
        };
        assert_eq!(
            allowed()
                .put(&upload, store.open_read(&staged).await?)
                .await?,
            PutOutcome::AlreadyStored
        );
    }

    fn short_timeout() -> Transfer {
        Transfer::with_timeouts(
            AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            },
            Duration::from_secs(1),
            Duration::from_millis(400),
        )
        .unwrap()
    }

    // verifies: ATCH-070
    #[xmtp_common::test(unwrap_try = true)]
    async fn connect_timeout_is_network() {
        let transfer = Transfer::with_resolver_and_timeouts(
            AttachmentOptions::default(),
            Arc::new(PendingResolver),
            Duration::from_millis(100),
            Duration::from_millis(400),
        )?;
        let started = Instant::now();
        let error = tokio::time::timeout(
            Duration::from_secs(2),
            transfer.get(
                "https://pending.example/object",
                1,
                &mut MemorySink::default(),
            ),
        )
        .await?
        .unwrap_err();
        assert_eq!(error.cause, Cause::Network);
        assert!(started.elapsed() < Duration::from_secs(2));

        let directory = tempfile::tempdir()?;
        let store = NativeStore::new(directory.path()).await?;
        let mut writer = store.create_temp(".tmp/body").await?;
        writer.write(b"body").await?;
        store.sync(&mut writer).await?;
        drop(writer);
        store.rename(".tmp/body", "body").await?;
        let request = UploadRequest {
            method: "PUT".into(),
            url: "https://pending.example/object".into(),
            headers: vec![],
            expires_in_seconds: 60,
        };
        let error = tokio::time::timeout(
            Duration::from_secs(2),
            transfer.put(&request, store.open_read("body").await?),
        )
        .await?
        .unwrap_err();
        assert_eq!(error.cause, Cause::Network);
    }

    // verifies: ATCH-070
    #[xmtp_common::test(unwrap_try = true)]
    async fn put_stalls_after_body() {
        use http_body_util::BodyExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let (body_received, received) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let sender = Arc::new(std::sync::Mutex::new(Some(body_received)));
            let service = service_fn(move |request: Request<Incoming>| {
                let sender = sender.clone();
                async move {
                    let body = request.into_body().collect().await.unwrap().to_bytes();
                    assert_eq!(&body[..], b"upload body");
                    sender.lock().unwrap().take().unwrap().send(()).unwrap();
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    Ok::<_, std::convert::Infallible>(answer(StatusCode::OK, ""))
                }
            });
            let _ = http1::Builder::new()
                .serve_connection(TokioIo::new(socket), service)
                .await;
        });
        let directory = tempfile::tempdir()?;
        let store = NativeStore::new(directory.path()).await?;
        let mut writer = store.create_temp(".tmp/body").await?;
        writer.write(b"upload body").await?;
        store.sync(&mut writer).await?;
        drop(writer);
        store.rename(".tmp/body", "body").await?;
        let request = UploadRequest {
            method: "PUT".into(),
            url,
            headers: vec![],
            expires_in_seconds: 60,
        };
        let error = short_timeout()
            .put(&request, store.open_read("body").await?)
            .await
            .unwrap_err();
        assert_eq!(error.cause, Cause::Network);
        tokio::time::timeout(Duration::from_secs(1), received).await??;
        task.abort();
        let _ = task.await;
    }

    // verifies: ATCH-070
    #[xmtp_common::test(unwrap_try = true)]
    async fn get_stalls_after_bytes() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\na")
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_secs(2)).await;
        });
        let mut sink = MemorySink::default();
        let error = short_timeout().get(&url, 5, &mut sink).await.unwrap_err();
        assert_eq!(error.cause, Cause::Network);
        assert_eq!(sink.0, b"a");
        task.abort();
        let _ = task.await;
    }

    // verifies: ATCH-070
    #[xmtp_common::test(unwrap_try = true)]
    async fn get_trickle_has_no_total_deadline() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n")
                .await
                .unwrap();
            for byte in b"abcde" {
                tokio::time::sleep(Duration::from_millis(150)).await;
                socket.write_all(&[*byte]).await.unwrap();
            }
        });
        let mut sink = MemorySink::default();
        short_timeout().get(&url, 5, &mut sink).await?;
        assert_eq!(sink.0, b"abcde");
        task.await?;
    }

    // verifies: ATCH-070
    #[xmtp_common::test(unwrap_try = true)]
    async fn put_trickle_has_no_total_deadline() {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
        const BODY_SIZE: usize = 3 * 1024 * 1024;
        let socket = tokio::net::TcpSocket::new_v4()?;
        socket.set_recv_buffer_size(8192)?;
        socket.bind("127.0.0.1:0".parse()?)?;
        let listener = socket.listen(1)?;
        let url = format!("http://{}", listener.local_addr()?);
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(socket);
            let mut line = String::new();
            let mut content_length = None;
            loop {
                line.clear();
                reader.read_line(&mut line).await.unwrap();
                if line == "\r\n" {
                    break;
                }
                let lower = line.to_ascii_lowercase();
                if let Some(length) = lower.strip_prefix("content-length: ") {
                    content_length = Some(length.trim().parse::<usize>().unwrap());
                }
            }
            assert_eq!(content_length, Some(BODY_SIZE));
            let mut remaining = BODY_SIZE;
            let mut tail_started = false;
            let mut buffer = [0_u8; 8192];
            while remaining > 0 {
                if remaining <= 512 * 1024 {
                    if !tail_started {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        tail_started = true;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                let limit = remaining.min(buffer.len());
                let size = reader.read(&mut buffer[..limit]).await.unwrap();
                assert!(size > 0);
                assert!(buffer[..size].iter().all(|byte| *byte == 0x5a));
                remaining -= size;
            }
            reader
                .get_mut()
                .write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });
        let directory = tempfile::tempdir()?;
        let store = NativeStore::new(directory.path()).await?;
        let mut writer = store.create_temp(".tmp/body").await?;
        for _ in 0..BODY_SIZE / CHUNK_SIZE {
            writer.write(&[0x5a; CHUNK_SIZE]).await?;
        }
        store.sync(&mut writer).await?;
        drop(writer);
        store.rename(".tmp/body", "body").await?;
        let request = UploadRequest {
            method: "PUT".into(),
            url,
            headers: vec![],
            expires_in_seconds: 60,
        };
        let started = Instant::now();
        let transfer = Transfer::with_timeouts(
            AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            },
            Duration::from_secs(1),
            Duration::from_secs(1),
        )?;
        assert_eq!(
            transfer
                .put(&request, store.open_read("body").await?)
                .await?,
            PutOutcome::Stored
        );
        assert!(started.elapsed() > Duration::from_secs(1));
        task.await?;
    }
}
