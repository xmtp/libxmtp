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
#[derive(Clone, Debug)]
pub struct UploadRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub expires_in_seconds: u32,
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
        Err(AttachmentError::new(Cause::TargetRejected))
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

pub(crate) fn sensitive_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "authorization" | "cookie" | "proxy-authorization"
    ) || name.starts_with("x-xmtp-")
        || name.starts_with("x-inbox-")
        || name.starts_with("x-installation-")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn credential_header_filter() {
        for name in [
            "Authorization",
            "Cookie",
            "Proxy-Authorization",
            "X-XMTP-Inbox-ID",
            "X-Inbox-Id",
            "X-Installation-Id",
        ] {
            assert!(sensitive_header(name));
        }
        assert!(!sensitive_header("Content-Type"));
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
