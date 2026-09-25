use std::{
    cell::{Cell, RefCell},
    net::IpAddr,
    rc::Rc,
    time::Duration,
};

use futures_util::{
    AsyncReadExt,
    future::{Either, select},
};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortController, ReferrerPolicy, Request, RequestCredentials, RequestInit, RequestRedirect,
    Response, ResponseType, WorkerGlobalScope,
};

use super::{
    IDLE_TIMEOUT, PutOutcome, UploadRequest, checked_count, put_outcome, sensitive_header,
};
use crate::{
    AttachmentError, AttachmentFailureCause as Cause,
    address::is_private,
    store::{AttachmentOptions, CHUNK_SIZE, DownloadSink, StagedFile},
};

fn validate_download_url(url: &str, options: &AttachmentOptions) -> Result<(), AttachmentError> {
    let url = url::Url::parse(url).map_err(|_| AttachmentError::new(Cause::InsecureUrl))?;
    let host = url.host().ok_or(AttachmentError::new(Cause::InsecureUrl))?;
    let loopback = match host {
        url::Host::Domain(name) => name.eq_ignore_ascii_case("localhost"),
        url::Host::Ipv4(ip) => ip.is_loopback(),
        url::Host::Ipv6(ip) => ip.is_loopback(),
    };
    if !options.allow_private_network
        && match host {
            url::Host::Domain(name) => is_localhost_name(name),
            url::Host::Ipv4(ip) => is_private(IpAddr::V4(ip)),
            url::Host::Ipv6(ip) => is_private(IpAddr::V6(ip)),
        }
    {
        return Err(AttachmentError::new(Cause::BlockedAddress));
    }
    if url.scheme() != "https"
        && !(url.scheme() == "http" && loopback && options.allow_private_network)
    {
        return Err(AttachmentError::new(Cause::InsecureUrl));
    }
    Ok(())
}

fn is_localhost_name(name: &str) -> bool {
    let name = name.strip_suffix('.').unwrap_or(name);
    name.eq_ignore_ascii_case("localhost")
        || name
            .rsplit_once('.')
            .is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case("localhost"))
}

fn validate_upload_url(url: &str) -> Result<(), AttachmentError> {
    let url = url::Url::parse(url).map_err(|_| AttachmentError::new(Cause::InsecureUrl))?;
    let loopback = match url.host() {
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    if url.scheme() == "https" || (url.scheme() == "http" && loopback) {
        Ok(())
    } else {
        Err(AttachmentError::new(Cause::InsecureUrl))
    }
}

async fn fetch(request: &Request) -> Result<Response, AttachmentError> {
    let global: WorkerGlobalScope = js_sys::global().unchecked_into();
    JsFuture::from(global.fetch_with_request(request))
        .await
        .map_err(|_| AttachmentError::new(Cause::Network))?
        .dyn_into()
        .map_err(|_| AttachmentError::new(Cause::Network))
}

fn set_upload_headers(request: &Request, upload: &UploadRequest) -> Result<(), AttachmentError> {
    for (name, value) in &upload.headers {
        if sensitive_header(name) {
            return Err(AttachmentError::new(Cause::Credential));
        }
        request
            .headers()
            .set(name, value)
            .map_err(|_| AttachmentError::new(Cause::Malformed))?;
    }
    Ok(())
}

async fn blob_put(
    upload: &UploadRequest,
    file: &web_sys::File,
) -> Result<PutOutcome, AttachmentError> {
    let init = private_request("PUT");
    init.set_body_opt_blob(Some(file));
    let request = Request::new_with_str_and_init(&upload.url, &init)
        .map_err(|_| AttachmentError::new(Cause::Malformed))?;
    set_upload_headers(&request, upload)?;
    let response = fetch(&request).await?;
    if response.type_() == ResponseType::Opaqueredirect {
        return Err(AttachmentError::new(Cause::TargetRejected));
    }
    put_outcome(response.status())
}

fn private_request(method: &str) -> RequestInit {
    let init = RequestInit::new();
    init.set_method(method);
    init.set_redirect(RequestRedirect::Manual);
    init.set_credentials(RequestCredentials::Omit);
    init.set_referrer_policy(ReferrerPolicy::NoReferrer);
    init
}

fn download_status(status: u16, response_type: ResponseType) -> Result<(), AttachmentError> {
    if response_type == ResponseType::Opaqueredirect || (300..400).contains(&status) {
        return Err(AttachmentError::new(Cause::TooManyRedirects));
    }
    match status {
        200 => Ok(()),
        404 | 410 => Err(AttachmentError::new(Cause::NotFound)),
        _ => Err(AttachmentError::new(Cause::HttpStatus)),
    }
}

struct AbortDeadline {
    controller: AbortController,
    timer: Rc<RefCell<Option<gloo_timers::callback::Timeout>>>,
    fired: Rc<Cell<bool>>,
    expires_at: Cell<f64>,
}

fn now_ms() -> f64 {
    let global: WorkerGlobalScope = js_sys::global().unchecked_into();
    global
        .performance()
        .map_or_else(js_sys::Date::now, |performance| performance.now())
}

impl AbortDeadline {
    fn new() -> Result<Self, AttachmentError> {
        Ok(Self {
            controller: AbortController::new().map_err(|_| AttachmentError::new(Cause::Network))?,
            timer: Rc::new(RefCell::new(None)),
            fired: Rc::new(Cell::new(false)),
            expires_at: Cell::new(0.0),
        })
    }

    fn arm(&self, duration: Duration) {
        self.expires_at
            .set(now_ms() + duration.as_secs_f64() * 1000.0);
        let controller = self.controller.clone();
        let fired = self.fired.clone();
        let milliseconds = duration.as_millis().min(u32::MAX as u128) as u32;
        *self.timer.borrow_mut() = Some(gloo_timers::callback::Timeout::new(
            milliseconds,
            move || {
                fired.set(true);
                controller.abort();
            },
        ));
    }

    fn stop(&self) {
        self.timer.borrow_mut().take();
    }
}

async fn read_body<R: futures_util::io::AsyncRead + Unpin>(
    reader: &mut R,
    cap: u64,
    sink: &mut dyn DownloadSink,
    deadline: &AbortDeadline,
    idle_timeout: Duration,
) -> Result<(), AttachmentError> {
    let mut buffer = [0_u8; CHUNK_SIZE];
    let mut count = 0;
    loop {
        let remaining = deadline.expires_at.get() - now_ms();
        if remaining <= 0.0 || deadline.fired.get() {
            deadline.controller.abort();
            return Err(AttachmentError::new(Cause::Network));
        }
        let milliseconds = remaining.ceil().min(u32::MAX as f64) as u32;
        let read = reader.read(&mut buffer);
        let timer = gloo_timers::future::TimeoutFuture::new(milliseconds);
        let size = match select(read, timer).await {
            Either::Left((result, _)) => {
                result.map_err(|_| AttachmentError::new(Cause::Network))?
            }
            Either::Right(_) => {
                deadline.controller.abort();
                return Err(AttachmentError::new(Cause::Network));
            }
        };
        if deadline.fired.get() {
            return Err(AttachmentError::new(Cause::Network));
        }
        if size == 0 {
            break;
        }
        deadline.arm(idle_timeout);
        count = checked_count(count, size, cap)?;
        sink.write(&buffer[..size]).await?;
    }
    deadline.stop();
    Ok(())
}

/// Browser transfer through fetch. Browser downloads reject redirects.
pub struct Transfer {
    options: AttachmentOptions,
    idle_timeout: Duration,
}

impl Transfer {
    pub fn new(options: AttachmentOptions) -> Result<Self, AttachmentError> {
        Ok(Self {
            options,
            idle_timeout: IDLE_TIMEOUT,
        })
    }

    #[cfg(test)]
    fn with_timeout(options: AttachmentOptions, idle_timeout: Duration) -> Self {
        Self {
            options,
            idle_timeout,
        }
    }

    pub async fn put(
        &self,
        upload: &UploadRequest,
        body: StagedFile,
    ) -> Result<PutOutcome, AttachmentError> {
        if upload.method != "PUT" {
            return Err(AttachmentError::new(Cause::TargetRejected));
        }
        validate_upload_url(&upload.url)?;
        blob_put(upload, &body.file).await
    }

    pub async fn get(
        &self,
        url: &str,
        cap: u64,
        sink: &mut dyn DownloadSink,
    ) -> Result<(), AttachmentError> {
        validate_download_url(url, &self.options)?;
        let deadline = AbortDeadline::new()?;
        deadline.arm(self.idle_timeout);
        let init = private_request("GET");
        init.set_signal(Some(&deadline.controller.signal()));
        let request = Request::new_with_str_and_init(url, &init)
            .map_err(|_| AttachmentError::new(Cause::InsecureUrl))?;
        let response = fetch(&request).await;
        let response = response?;
        if deadline.fired.get() {
            return Err(AttachmentError::new(Cause::Network));
        }
        download_status(response.status(), response.type_())?;
        let body = response
            .body()
            .ok_or(AttachmentError::new(Cause::HttpStatus))?;
        let mut reader = wasm_streams::ReadableStream::from_raw(body).into_async_read();
        let cap = cap.min(self.options.max_download_bytes.unwrap_or(u64::MAX));
        read_body(&mut reader, cap, sink, &deadline, self.idle_timeout).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    struct PendingReader;

    impl futures_util::io::AsyncRead for PendingReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            Poll::Pending
        }
    }

    struct NullSink;

    #[async_trait::async_trait(?Send)]
    impl DownloadSink for NullSink {
        async fn write(&mut self, _bytes: &[u8]) -> Result<(), AttachmentError> {
            Ok(())
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn request_omits_browser_credentials() {
        for method in ["GET", "PUT"] {
            let init = private_request(method);
            assert_eq!(init.get_credentials(), Some(RequestCredentials::Omit));
            assert_eq!(init.get_referrer_policy(), Some(ReferrerPolicy::NoReferrer));
        }
    }

    // verifies: ATCH-071
    #[xmtp_common::test(unwrap_try = true)]
    fn upload_url_policy() {
        for url in [
            "https://10.1.2.3/object",
            "http://localhost/object",
            "http://127.0.0.1/object",
            "http://[::1]/object",
        ] {
            validate_upload_url(url)?;
        }
        for url in ["http://example.com/object", "ftp://localhost/object"] {
            assert_eq!(
                validate_upload_url(url).unwrap_err().cause,
                Cause::InsecureUrl
            );
        }
    }

    // verifies: ATCH-071
    #[xmtp_common::test(unwrap_try = true)]
    fn put_redirect_is_manual() {
        assert_eq!(
            private_request("PUT").get_redirect(),
            Some(RequestRedirect::Manual)
        );
        assert_eq!(put_outcome(307).unwrap_err().cause, Cause::TargetRejected);
    }

    // verifies: ATCH-054
    #[xmtp_common::test(unwrap_try = true)]
    async fn private_download_hosts_are_blocked_before_fetch() {
        let transfer = Transfer::new(AttachmentOptions::default())?;
        for url in [
            "https://10.0.0.1/object",
            "https://[::1]/object",
            "https://LOCALHOST/object",
            "http://LOCALHOST/object",
            "https://localhost./object",
            "https://a.localhost/object",
            "https://A.LOCALHOST./object",
            "https://[::ffff:10.0.0.1]/object",
            "https://[64:ff9b::a00:1]/object",
        ] {
            assert_eq!(
                transfer.get(url, 1, &mut NullSink).await.unwrap_err().cause,
                Cause::BlockedAddress,
                "{url}"
            );
        }
        validate_download_url("https://example.com/object", &AttachmentOptions::default())?;
        let options = AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        };
        validate_download_url("https://10.0.0.1/object", &options)?;
        validate_download_url("https://LOCALHOST/object", &options)?;
        validate_download_url("http://localhost/object", &options)?;
    }

    // verifies: ATCH-055
    #[xmtp_common::test(unwrap_try = true)]
    fn download_redirect_is_rejected() {
        assert_eq!(
            private_request("GET").get_redirect(),
            Some(RequestRedirect::Manual)
        );
        for (status, response_type) in [
            (0, ResponseType::Opaqueredirect),
            (301, ResponseType::Basic),
            (307, ResponseType::Basic),
        ] {
            assert_eq!(
                download_status(status, response_type).unwrap_err().cause,
                Cause::TooManyRedirects
            );
        }
        download_status(200, ResponseType::Basic)?;
    }

    // verifies: ATCH-070
    #[xmtp_common::test(unwrap_try = true)]
    async fn deadline_resets_on_progress() {
        let transfer =
            Transfer::with_timeout(AttachmentOptions::default(), Duration::from_millis(80));
        let deadline = AbortDeadline::new()?;
        deadline.arm(transfer.idle_timeout);
        gloo_timers::future::TimeoutFuture::new(50).await;
        deadline.arm(transfer.idle_timeout);
        gloo_timers::future::TimeoutFuture::new(50).await;
        assert!(!deadline.controller.signal().aborted());
        gloo_timers::future::TimeoutFuture::new(60).await;
        assert!(deadline.controller.signal().aborted());
        assert!(deadline.fired.get());
    }

    // verifies: ATCH-070
    #[xmtp_common::test(unwrap_try = true)]
    async fn get_read_loop_aborts_idle_body() {
        let deadline = AbortDeadline::new()?;
        deadline.arm(Duration::from_millis(40));
        let error = read_body(
            &mut PendingReader,
            100,
            &mut NullSink,
            &deadline,
            Duration::from_millis(40),
        )
        .await
        .unwrap_err();
        assert_eq!(error.cause, Cause::Network);
        assert!(deadline.controller.signal().aborted());
    }
}
