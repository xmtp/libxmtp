use std::sync::atomic::{AtomicU8, Ordering};

/// Metadata about a log event variant, including its doc comment and required context fields.
/// This struct is used by proc macros to access event metadata at compile time.
#[derive(Debug, Clone, Copy)]
pub struct EventMetadata {
    /// The name of the enum variant
    pub name: &'static str,
    /// The doc comment describing the event
    pub doc: &'static str,
    /// The required context fields for this event
    pub context_fields: &'static [&'static str],
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
