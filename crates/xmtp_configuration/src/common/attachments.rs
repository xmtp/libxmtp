//! Checks for attachment storage configuration shared by the backend and client.

use std::net::{Ipv4Addr, Ipv6Addr};

use super::server::MAX_PUBLISHED_VALUE;

/// Default maximum ciphertext size for one attachment upload, in bytes.
pub const BACKEND_DEFAULT_MAX_UPLOAD_BYTES: u64 = 104_857_600;
/// Largest attachment `contentLength` the content type can carry.
pub const MAX_ATTACHMENT_UPLOAD_BYTES: u64 = u32::MAX as u64;
/// Largest retention value an SDK integer can carry exactly.
pub const MAX_ATTACHMENT_RETENTION_SECONDS: u64 = MAX_PUBLISHED_VALUE;

/// An attachment configuration field and the reason it cannot be used.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AttachmentConfigurationError {
    #[error("base_url: {reason}")]
    BaseUrl { reason: &'static str },
    #[error("max_upload_bytes: {reason}")]
    MaxUploadBytes { reason: &'static str },
    #[error("retention_seconds: {reason}")]
    RetentionSeconds { reason: &'static str },
}

impl AttachmentConfigurationError {
    pub const fn field(&self) -> &'static str {
        match self {
            Self::BaseUrl { .. } => "base_url",
            Self::MaxUploadBytes { .. } => "max_upload_bytes",
            Self::RetentionSeconds { .. } => "retention_seconds",
        }
    }

    pub const fn reason(&self) -> &'static str {
        match self {
            Self::BaseUrl { reason }
            | Self::MaxUploadBytes { reason }
            | Self::RetentionSeconds { reason } => reason,
        }
    }
}

fn invalid_url(reason: &'static str) -> AttachmentConfigurationError {
    AttachmentConfigurationError::BaseUrl { reason }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_sub_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn valid_path(path: &str) -> bool {
    let mut bytes = path.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            if !bytes.next().is_some_and(|digit| digit.is_ascii_hexdigit())
                || !bytes.next().is_some_and(|digit| digit.is_ascii_hexdigit())
            {
                return false;
            }
        } else if !(is_unreserved(byte)
            || is_sub_delimiter(byte)
            || matches!(byte, b'/' | b':' | b'@'))
        {
            return false;
        }
    }
    true
}

fn is_dot_segment(segment: &str) -> bool {
    let mut bytes = segment.bytes();
    let mut dots = 0;
    while let Some(byte) = bytes.next() {
        if byte == b'.'
            || (byte == b'%'
                && matches!(bytes.next(), Some(b'2'))
                && matches!(bytes.next(), Some(b'e' | b'E')))
        {
            dots += 1;
        } else {
            return false;
        }
    }
    dots == 1 || dots == 2
}

/// Check the published base URL without replacing its raw host, port, or path.
// implements: ATCH-002, ATCH-008
pub fn check_base_url(base_url: &str) -> Result<(), AttachmentConfigurationError> {
    if base_url.bytes().any(|byte| {
        !byte.is_ascii() || byte.is_ascii_control() || byte.is_ascii_whitespace() || byte == b'\\'
    }) {
        return Err(invalid_url(
            "contains non-ASCII, whitespace, control, or backslash",
        ));
    }

    let (scheme, authority_and_path) = base_url
        .split_once("://")
        .ok_or_else(|| invalid_url("is not an absolute URL with an authority"))?;
    let is_http = scheme.eq_ignore_ascii_case("http");
    if !is_http && !scheme.eq_ignore_ascii_case("https") {
        return Err(invalid_url("scheme is not https or loopback http"));
    }
    if base_url.contains('?') || base_url.contains('#') {
        return Err(invalid_url("has a query or fragment"));
    }
    if base_url.ends_with('/') {
        return Err(invalid_url("has a trailing slash"));
    }

    let authority_end = authority_and_path
        .find('/')
        .unwrap_or(authority_and_path.len());
    let (authority, path) = authority_and_path.split_at(authority_end);
    if authority.is_empty() {
        return Err(invalid_url("has no host"));
    }
    if authority.contains('@') {
        return Err(invalid_url("has userinfo"));
    }
    if authority.contains('%') {
        return Err(invalid_url("has an encoded host"));
    }

    let (host, port, is_ipv6) = if let Some(bracketed) = authority.strip_prefix('[') {
        let (host, suffix) = bracketed
            .split_once(']')
            .ok_or_else(|| invalid_url("has an invalid IP literal"))?;
        let port = if suffix.is_empty() {
            None
        } else {
            Some(
                suffix
                    .strip_prefix(':')
                    .ok_or_else(|| invalid_url("has an invalid port"))?,
            )
        };
        (host, port, true)
    } else {
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        (host, port, false)
    };
    if port.is_some_and(|port| {
        !port.bytes().all(|byte| byte.is_ascii_digit())
            || (!port.is_empty() && port.parse::<u16>().is_err())
    }) {
        return Err(invalid_url("has an invalid port"));
    }

    let is_loopback = if is_ipv6 {
        host.parse::<Ipv6Addr>()
            .map_err(|_| invalid_url("has an invalid IP literal"))?
            .is_loopback()
    } else {
        if host.is_empty()
            || !host
                .bytes()
                .all(|byte| is_unreserved(byte) || is_sub_delimiter(byte))
        {
            return Err(invalid_url("has an invalid host"));
        }
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<Ipv4Addr>()
                .is_ok_and(|address| address.is_loopback())
    };
    if is_http && !is_loopback {
        return Err(invalid_url(
            "http host is not localhost or a loopback IP literal",
        ));
    }
    if !valid_path(path) || path.split('/').any(is_dot_segment) {
        return Err(invalid_url("has an invalid or changing path"));
    }
    Ok(())
}

/// Check the effective upload limit after a missing wire value takes its default.
// implements: ATCH-003, ATCH-008
pub fn check_max_upload_bytes(value: u64) -> Result<(), AttachmentConfigurationError> {
    if value == 0 || value > MAX_ATTACHMENT_UPLOAD_BYTES {
        return Err(AttachmentConfigurationError::MaxUploadBytes {
            reason: "must be from 1 through the largest attachment contentLength",
        });
    }
    Ok(())
}

/// Check a configured or published retention value.
// implements: ATCH-004, ATCH-008
pub fn check_retention_seconds(value: u64) -> Result<(), AttachmentConfigurationError> {
    if value > MAX_ATTACHMENT_RETENTION_SECONDS {
        return Err(AttachmentConfigurationError::RetentionSeconds {
            reason: "exceeds the largest exact SDK integer",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
