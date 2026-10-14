//! Handles: short public names, such as `nick` or `nick#2341`, that an inbox
//! claims in its identity log.
//!
//! This module holds only the grammar and the limits every validator applies.
//! Policy that only the backend applies, such as the reserved names and the
//! time between claims, lives with the backend.

use regex::Regex;
use std::{fmt, sync::LazyLock};
use xmtp_common::ErrorCode;

/// The most characters a handle's name part can hold.
pub const MAX_HANDLE_NAME_LEN: usize = 27;

/// The most `ClaimHandle` actions one association log can hold.
pub const MAX_HANDLE_CLAIMS: u32 = 64;

/// One more than the highest four-digit code.
const CODE_LIMIT: u16 = 10_000;

/// The handle grammar: a name part of 1 to [`MAX_HANDLE_NAME_LEN`] characters
/// that starts with a lowercase ASCII letter, optionally followed by `#` and
/// exactly four ASCII digits. `$` matches only at the end of the text, so a
/// trailing newline does not match.
static GRAMMAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-z][a-z0-9_-]{0,26}(#[0-9]{4})?$").expect("static regex should always compile")
});

/// Invalid handle.
///
/// The string is not a handle under the handle grammar. Not retryable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, ErrorCode)]
#[error("the string is not a handle under the handle grammar")]
pub struct InvalidHandle;

/// A string that the handle grammar matches in full. It is the one spelling
/// of the handle, in lowercase.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Handle(String);

impl Handle {
    /// Accepts `canonical` only when the grammar matches it in full. A claim's
    /// handle is part of the signed text, so this never rewrites it: `Nick`
    /// is rejected, not read as `nick`.
    pub fn parse(canonical: &str) -> Result<Self, InvalidHandle> {
        if GRAMMAR.is_match(canonical) {
            Ok(Self(canonical.to_owned()))
        } else {
            Err(InvalidHandle)
        }
    }

    /// Converts the ASCII uppercase letters of `input` to lowercase, then
    /// parses the result. No other character changes, so input that is not
    /// ASCII is still rejected. Use this for text a user typed, never for a
    /// signed claim.
    pub fn from_input(input: &str) -> Result<Self, InvalidHandle> {
        Self::parse(&input.to_ascii_lowercase())
    }

    /// The handle with the name part `name` and the code `code`, written as
    /// four digits with leading zeros. Fails when `code` has more than four
    /// digits or `name` is not a name part.
    pub fn with_code(name: &str, code: u16) -> Result<Self, InvalidHandle> {
        if code >= CODE_LIMIT {
            return Err(InvalidHandle);
        }
        Self::parse(&format!("{name}#{code:04}"))
    }

    /// The handle text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The name part: the handle without `#` and its code.
    pub fn name(&self) -> &str {
        self.0.split_once('#').map_or(&self.0, |(name, _)| name)
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Handle {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<Handle> for String {
    fn from(handle: Handle) -> Self {
        handle.0
    }
}

#[cfg(test)]
mod tests;
