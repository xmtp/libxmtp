use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use futures_util::{AsyncReadExt, Stream};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortController, ReferrerPolicy, Request, RequestCredentials, RequestInit, RequestRedirect,
    Response, WorkerGlobalScope,
};

use super::{
    CONNECT_TIMEOUT, IDLE_TIMEOUT, PutOutcome, UploadRequest, checked_count, put_outcome,
    sensitive_header,
};
use crate::{
    AttachmentError, AttachmentFailureCause as Cause,
    store::{AttachmentOptions, CHUNK_SIZE, DownloadSink, StagedFile},
};

fn validate_url(url: &str, options: &AttachmentOptions) -> Result<(), AttachmentError> {
    let url = url::Url::parse(url).map_err(|_| AttachmentError::new(Cause::InsecureUrl))?;
    let loopback = match url.host() {
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    if url.scheme() == "https"
        || (url.scheme() == "http" && loopback && options.allow_private_network)
    {
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

// Blob bodies work in browsers that cannot send a request stream. Fetch does not
// report progress for a Blob body, so this path cannot apply an idle deadline.
async fn blob_put(
    upload: &UploadRequest,
    file: &web_sys::File,
) -> Result<PutOutcome, AttachmentError> {
    let init = private_request("PUT");
    init.set_body_opt_blob(Some(file));
    let request = Request::new_with_str_and_init(&upload.url, &init)
        .map_err(|_| AttachmentError::new(Cause::Malformed))?;
    set_upload_headers(&request, upload)?;
    put_outcome(fetch(&request).await?.status())
}

fn private_request(method: &str) -> RequestInit {
    let init = RequestInit::new();
    init.set_method(method);
    init.set_redirect(RequestRedirect::Follow);
    init.set_credentials(RequestCredentials::Omit);
    init.set_referrer_policy(ReferrerPolicy::NoReferrer);
    init
}

#[derive(Clone)]
struct AbortDeadline {
    controller: AbortController,
    timer: Rc<RefCell<Option<gloo_timers::callback::Timeout>>>,
    fired: Rc<Cell<bool>>,
}

impl AbortDeadline {
    fn new() -> Result<Self, AttachmentError> {
        Ok(Self {
            controller: AbortController::new().map_err(|_| AttachmentError::new(Cause::Network))?,
            timer: Rc::new(RefCell::new(None)),
            fired: Rc::new(Cell::new(false)),
        })
    }

    fn arm(&self, duration: Duration) {
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

/// Browser transfer through fetch. The browser owns DNS and redirects.
pub struct Transfer {
    options: AttachmentOptions,
    connect_timeout: Duration,
    idle_timeout: Duration,
}

impl Transfer {
    pub fn new(options: AttachmentOptions) -> Result<Self, AttachmentError> {
        Ok(Self {
            options,
            connect_timeout: CONNECT_TIMEOUT,
            idle_timeout: IDLE_TIMEOUT,
        })
    }

    #[cfg(test)]
    fn with_timeouts(
        options: AttachmentOptions,
        connect_timeout: Duration,
        idle_timeout: Duration,
    ) -> Self {
        Self {
            options,
            connect_timeout,
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
        validate_url(&upload.url, &self.options)?;
        if upload
            .headers
            .iter()
            .any(|(name, _)| sensitive_header(name))
        {
            return Err(AttachmentError::new(Cause::Credential));
        }
        let deadline = AbortDeadline::new()?;
        deadline.arm(self.connect_timeout);
        let init = private_request("PUT");
        init.set_signal(Some(&deadline.controller.signal()));
        let progress = deadline.clone();
        let idle = self.idle_timeout;
        let mut source =
            Box::pin(wasm_streams::ReadableStream::from_raw(body.file.stream()).into_stream());
        let stream =
            wasm_streams::ReadableStream::from_stream(futures_util::stream::poll_fn(move |cx| {
                let next = source.as_mut().poll_next(cx);
                if matches!(
                    &next,
                    std::task::Poll::Ready(Some(Ok(_))) | std::task::Poll::Ready(None)
                ) {
                    progress.arm(idle);
                }
                next
            }))
            .into_raw();
        init.set_body_opt_readable_stream(Some(&stream));
        js_sys::Reflect::set(init.as_ref(), &"duplex".into(), &"half".into())
            .map_err(|_| AttachmentError::new(Cause::Malformed))?;
        let request = match Request::new_with_str_and_init(&upload.url, &init) {
            Ok(request) => request,
            Err(_) => {
                deadline.stop();
                return blob_put(upload, &body.file).await;
            }
        };
        if request
            .headers()
            .get("content-type")
            .ok()
            .flatten()
            .is_some_and(|value| value.starts_with("text/plain"))
        {
            deadline.stop();
            return blob_put(upload, &body.file).await;
        }
        set_upload_headers(&request, upload)?;
        let response = fetch(&request).await;
        deadline.stop();
        let response = match response {
            Ok(response) => response,
            Err(error) if deadline.fired.get() => return Err(error),
            Err(_) => return blob_put(upload, &body.file).await,
        };
        put_outcome(response.status())
    }

    pub async fn get(
        &self,
        url: &str,
        cap: u64,
        sink: &mut dyn DownloadSink,
    ) -> Result<(), AttachmentError> {
        validate_url(url, &self.options)?;
        let deadline = AbortDeadline::new()?;
        deadline.arm(self.connect_timeout);
        let init = private_request("GET");
        init.set_signal(Some(&deadline.controller.signal()));
        let request = Request::new_with_str_and_init(url, &init)
            .map_err(|_| AttachmentError::new(Cause::InsecureUrl))?;
        let response = fetch(&request).await;
        deadline.stop();
        let response = response?;
        deadline.arm(self.idle_timeout);
        match response.status() {
            200 => {}
            404 | 410 => return Err(AttachmentError::new(Cause::NotFound)),
            _ => return Err(AttachmentError::new(Cause::HttpStatus)),
        }
        let body = response
            .body()
            .ok_or(AttachmentError::new(Cause::HttpStatus))?;
        let mut reader = wasm_streams::ReadableStream::from_raw(body).into_async_read();
        let mut buffer = [0_u8; CHUNK_SIZE];
        let mut count = 0;
        let cap = cap.min(self.options.max_download_bytes.unwrap_or(u64::MAX));
        loop {
            let size = reader.read(&mut buffer).await.map_err(|_| {
                deadline.stop();
                AttachmentError::new(Cause::Network)
            })?;
            if size == 0 {
                break;
            }
            deadline.stop();
            count = checked_count(count, size, cap)?;
            sink.write(&buffer[..size]).await?;
            deadline.arm(self.idle_timeout);
        }
        deadline.stop();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn request_omits_browser_credentials() {
        for method in ["GET", "PUT"] {
            let init = private_request(method);
            assert_eq!(init.get_credentials(), Some(RequestCredentials::Omit));
            assert_eq!(init.get_referrer_policy(), Some(ReferrerPolicy::NoReferrer));
        }
    }

    // verifies: ATCH-070
    #[xmtp_common::test(unwrap_try = true)]
    async fn deadline_resets_on_progress() {
        let transfer = Transfer::with_timeouts(
            AttachmentOptions::default(),
            Duration::from_millis(80),
            Duration::from_millis(80),
        );
        let deadline = AbortDeadline::new()?;
        deadline.arm(transfer.connect_timeout);
        gloo_timers::future::TimeoutFuture::new(50).await;
        deadline.arm(transfer.idle_timeout);
        gloo_timers::future::TimeoutFuture::new(50).await;
        assert!(!deadline.controller.signal().aborted());
        gloo_timers::future::TimeoutFuture::new(60).await;
        assert!(deadline.controller.signal().aborted());
        assert!(deadline.fired.get());
    }
}
