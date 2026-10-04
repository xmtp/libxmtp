// Keep these declarations in one module so generated binding paths stay stable.
include!("content/records.rs");
include!("content/standard.rs");
include!("content/envelope.rs");
#[cfg(any(test, feature = "conformance"))]
#[path = "content/pure_codec_tests.rs"]
pub(crate) mod pure_codec_tests;
include!("content/conformance.rs");

#[cfg(test)]
#[path = "content/diagnostics_tests.rs"]
mod diagnostics_tests;
