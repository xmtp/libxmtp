use futures_util::AsyncReadExt;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    ReferrerPolicy, Request, RequestCredentials, RequestInit, RequestRedirect, Response,
    WorkerGlobalScope,
};

use super::{PutOutcome, UploadRequest, checked_count, put_outcome, sensitive_header};
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

fn private_request(method: &str) -> RequestInit {
    let init = RequestInit::new();
    init.set_method(method);
    init.set_redirect(RequestRedirect::Follow);
    init.set_credentials(RequestCredentials::Omit);
    init.set_referrer_policy(ReferrerPolicy::NoReferrer);
    init
}

/// Browser transfer through fetch. The browser owns DNS and redirects.
pub struct Transfer {
    options: AttachmentOptions,
}

impl Transfer {
    pub fn new(options: AttachmentOptions) -> Result<Self, AttachmentError> {
        Ok(Self { options })
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
        let init = private_request("PUT");
        init.set_body_opt_blob(Some(&body.file));
        let request = Request::new_with_str_and_init(&upload.url, &init)
            .map_err(|_| AttachmentError::new(Cause::Malformed))?;
        for (name, value) in &upload.headers {
            if sensitive_header(name) {
                return Err(AttachmentError::new(Cause::Credential));
            }
            request
                .headers()
                .set(name, value)
                .map_err(|_| AttachmentError::new(Cause::Malformed))?;
        }
        put_outcome(fetch(&request).await?.status())
    }

    pub async fn get(
        &self,
        url: &str,
        cap: u64,
        sink: &mut dyn DownloadSink,
    ) -> Result<(), AttachmentError> {
        validate_url(url, &self.options)?;
        let init = private_request("GET");
        let request = Request::new_with_str_and_init(url, &init)
            .map_err(|_| AttachmentError::new(Cause::InsecureUrl))?;
        let response = fetch(&request).await?;
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
            let size = reader
                .read(&mut buffer)
                .await
                .map_err(|_| AttachmentError::new(Cause::Network))?;
            if size == 0 {
                break;
            }
            count = checked_count(count, size, cap)?;
            sink.write(&buffer[..size]).await?;
        }
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
}
