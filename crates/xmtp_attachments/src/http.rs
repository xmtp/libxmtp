//! Bounded transfer of staged ciphertext.

use std::time::Duration;

use crate::store::AttachmentOptions;
use crate::{AttachmentError, AttachmentFailureCause as Cause};

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(not(target_arch = "wasm32"))]
pub use native::Transfer;
#[cfg(target_arch = "wasm32")]
pub use wasm::Transfer;

/// Limit for establishing a connection to a storage target or download host.
#[cfg(not(target_arch = "wasm32"))]
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Limit between bytes during a storage transfer.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Values from the backend's signed upload response.
#[derive(Clone)]
pub struct UploadRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub expires_in_seconds: u32,
}

impl std::fmt::Debug for UploadRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let origin = url::Url::parse(&self.url)
            .map(|url| url.origin().ascii_serialization())
            .unwrap_or_else(|_| "<invalid>".to_owned());
        let header_names: Vec<&str> = self.headers.iter().map(|(name, _)| name.as_str()).collect();
        formatter
            .debug_struct("UploadRequest")
            .field("method", &self.method)
            .field("url", &origin)
            .field("headers", &header_names)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PutOutcome {
    Stored,
    AlreadyStored,
}

pub(crate) fn put_outcome(status: u16) -> Result<PutOutcome, AttachmentError> {
    if status == 412 {
        Ok(PutOutcome::AlreadyStored)
    } else if (200..300).contains(&status) {
        Ok(PutOutcome::Stored)
    } else {
        Err(AttachmentError::with_http_status(
            Cause::TargetRejected,
            status,
        ))
    }
}

/// Compute the bound before a download starts.
pub fn download_cap(
    content_length: Option<u64>,
    snapshot_max_upload_bytes: u64,
    options: &AttachmentOptions,
) -> u64 {
    content_length
        .unwrap_or(u64::MAX)
        .min(
            options
                .max_download_bytes
                .unwrap_or(snapshot_max_upload_bytes),
        )
        .min(u32::MAX as u64)
}

pub(crate) fn checked_count(current: u64, added: usize, cap: u64) -> Result<u64, AttachmentError> {
    let next = current
        .checked_add(added as u64)
        .ok_or(AttachmentError::new(Cause::TooLarge))?;
    if next > cap.min(u32::MAX as u64) {
        return Err(AttachmentError::new(Cause::TooLarge));
    }
    Ok(next)
}

/// Apply the backend upload URL policy on both targets.
pub(crate) fn secure_upload_url(url: &url::Url) -> Result<(), AttachmentError> {
    let host = url.host().ok_or(AttachmentError::new(Cause::InsecureUrl))?;
    let loopback = match host {
        url::Host::Domain(name) => is_loopback_name(name),
        url::Host::Ipv4(address) => address.is_loopback(),
        url::Host::Ipv6(address) => address.is_loopback(),
    };
    if url.scheme() == "https" || (url.scheme() == "http" && loopback) {
        Ok(())
    } else {
        Err(AttachmentError::new(Cause::InsecureUrl))
    }
}

pub(crate) fn is_loopback_name(name: &str) -> bool {
    name.strip_suffix('.')
        .unwrap_or(name)
        .eq_ignore_ascii_case("localhost")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn upload_request_debug_hides_signed_values() {
        let request = UploadRequest {
            method: "PUT".into(),
            url: "https://storage.example/path?X-Amz-Signature=query-secret".into(),
            headers: vec![("authorization".into(), "header-secret".into())],
            expires_in_seconds: 60,
        };
        let debug = format!("{request:?}");
        assert!(debug.contains("PUT"));
        assert!(debug.contains("https://storage.example"));
        assert!(debug.contains("authorization"));
        assert!(!debug.contains("query-secret"));
        assert!(!debug.contains("header-secret"));
        assert!(!debug.contains("/path"));
    }

    // verifies: ATCH-070
    #[xmtp_common::test(unwrap_try = true)]
    fn transfer_deadlines() {
        #[cfg(not(target_arch = "wasm32"))]
        assert_eq!(CONNECT_TIMEOUT, Duration::from_secs(30));
        assert_eq!(IDLE_TIMEOUT, Duration::from_secs(60));
    }

    // verifies: ATCH-056
    #[xmtp_common::test(unwrap_try = true)]
    fn size_cap_least_of() {
        let mut options = AttachmentOptions::default();
        assert_eq!(download_cap(Some(10), 100, &options), 10);
        assert_eq!(download_cap(None, 100, &options), 100);
        options.max_download_bytes = Some(20);
        assert_eq!(download_cap(Some(30), 100, &options), 20);
        options.max_download_bytes = Some(u64::MAX);
        assert_eq!(download_cap(None, u64::MAX, &options), u32::MAX as u64);
        assert!(checked_count(9, 2, 10).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn any_success_status_stores_upload() {
        for status in [200, 201, 202, 204, 206, 299] {
            assert_eq!(put_outcome(status)?, PutOutcome::Stored);
        }
        assert_eq!(put_outcome(412)?, PutOutcome::AlreadyStored);
        assert_eq!(put_outcome(500).unwrap_err().cause, Cause::TargetRejected);
    }
}
