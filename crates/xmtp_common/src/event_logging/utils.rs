use std::sync::atomic::{AtomicU8, Ordering};

use crate::Event;

/// Metadata about a log event variant, including its doc comment and required context fields.
/// This struct is used by proc macros to access event metadata at compile time.
#[derive(Debug, Clone, Copy)]
pub struct EventMetadata {
    /// The name of the enum variant
    pub name: &'static str,
    pub event: Event,
    /// The doc comment describing the event
    pub doc: &'static str,
    /// The required context fields for this event
    pub context_fields: &'static [&'static str],
}

impl EventMetadata {
    /// Validates that all required context fields are provided.
    /// Panics at compile time with the missing field name if validation fails.
    pub const fn validate_fields(&self, provided: &[&str]) {
        let mut i = 0;
        while i < self.context_fields.len() {
            let required = self.context_fields[i];
            if !str_contains(provided, required) {
                const_panic::concat_panic!(
                    "log_event! missing required context field: `",
                    required,
                    "`"
                );
            }
            i += 1;
        }
    }
}

const fn str_contains(haystack: &[&str], needle: &str) -> bool {
    let mut i = 0;
    while i < haystack.len() {
        if str_eq(haystack[i], needle) {
            return true;
        }
        i += 1;
    }
    false
}

const fn str_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const UNINITIALIZED: u8 = 0;
const STRUCTURED: u8 = 1;
const NOT_STRUCTURED: u8 = 2;

static STRUCTURED_LOGGING: AtomicU8 = AtomicU8::new(UNINITIALIZED);

/// Returns true if structured (JSON) logging is enabled.
/// When true, context should not be embedded in the message to avoid duplication.
/// Initializes from environment on first call, then caches the result.
#[inline]
pub fn is_structured_logging() -> bool {
    match STRUCTURED_LOGGING.load(Ordering::Relaxed) {
        STRUCTURED => true,
        NOT_STRUCTURED => false,
        _ => is_structured_logging_init(),
    }
}

#[cold]
fn is_structured_logging_init() -> bool {
    let is_structured = std::env::var("STRUCTURED").is_ok_and(|s| s == "true" || s == "1");
    STRUCTURED_LOGGING.store(
        if is_structured {
            STRUCTURED
        } else {
            NOT_STRUCTURED
        },
        Ordering::Relaxed,
    );
    is_structured
}
