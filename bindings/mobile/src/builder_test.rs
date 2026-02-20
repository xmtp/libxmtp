/// Tests for the `#[uniffi_builder]` macro.
///
/// UniFFI's `#[uniffi::export]` requires structs at crate-level scope (not inside
/// `mod tests {}`). The entire file is gated behind `#[cfg(test)]` in lib.rs.
use std::sync::Arc;

// -- Primitive types -------------------------------------------------------
#[xmtp_macro::uniffi_builder]
pub struct UniffiPrimitive {
    #[builder(required)]
    pub name: String,

    pub flag: Option<bool>,

    pub count: Option<u32>,
}

// -- Default values --------------------------------------------------------
#[allow(dead_code)]
#[xmtp_macro::uniffi_builder]
pub struct UniffiDefaults {
    #[builder(required)]
    pub id: u32,

    #[builder(default = "true")]
    pub enabled: bool,

    #[builder(default = "8080")]
    pub port: u32,
}

// -- Skip fields -----------------------------------------------------------
trait Callback: Send + Sync {
    fn invoke(&self) -> i32;
}

struct TestCallback;
impl Callback for TestCallback {
    fn invoke(&self) -> i32 {
        99
    }
}

#[allow(dead_code)]
#[xmtp_macro::uniffi_builder]
pub struct UniffiSkip {
    #[builder(required)]
    pub name: String,

    #[builder(skip)]
    internal: Vec<u8>,

    #[builder(skip)]
    callback: Option<Arc<dyn Callback>>,
}

// -- All modes combined ----------------------------------------------------
#[xmtp_macro::uniffi_builder]
pub struct UniffiMixed {
    #[builder(required)]
    pub host: String,

    #[builder(required)]
    pub port: u32,

    pub label: Option<String>,

    #[builder(default = "false")]
    pub debug: bool,

    #[builder(skip)]
    cache: Vec<String>,
}

// -- Tests -----------------------------------------------------------------

#[test]
fn test_primitive_constructor_and_setters() {
    let mut b = UniffiPrimitive::new("hello".to_string());
    assert_eq!(b.name, "hello");
    assert_eq!(b.flag, None);
    assert_eq!(b.count, None);

    b.flag(true);
    b.count(42);
    assert_eq!(b.flag, Some(true));
    assert_eq!(b.count, Some(42));
}

#[test]
fn test_setter_chaining() {
    let mut b = UniffiPrimitive::new("chain".to_string());
    b.flag(true).count(99);
    assert_eq!(b.flag, Some(true));
    assert_eq!(b.count, Some(99));
}

#[test]
fn test_defaults_applied_and_overridden() {
    let b = UniffiDefaults::new(1);
    assert!(b.enabled);
    assert_eq!(b.port, 8080);

    let mut b2 = UniffiDefaults::new(2);
    b2.enabled(false);
    b2.port(9090);
    assert!(!b2.enabled);
    assert_eq!(b2.port, 9090);
}

#[test]
fn test_skip_fields_initialized() {
    let mut b = UniffiSkip::new("test".to_string());
    assert!(b.internal.is_empty());
    assert!(b.callback.is_none());

    b.callback = Some(Arc::new(TestCallback));
    assert_eq!(b.callback.unwrap().invoke(), 99);
}

#[test]
fn test_mixed_modes() {
    let mut b = UniffiMixed::new("localhost".to_string(), 443);
    assert_eq!(b.host, "localhost");
    assert_eq!(b.port, 443);
    assert_eq!(b.label, None);
    assert!(!b.debug);
    assert!(b.cache.is_empty());

    b.label("production".to_string());
    b.debug(true);
    assert_eq!(b.label, Some("production".to_string()));
    assert!(b.debug);
}

#[test]
fn test_mixed_chaining() {
    let mut b = UniffiMixed::new("chain".to_string(), 80);
    b.label("test".to_string()).debug(true);
    assert_eq!(b.label, Some("test".to_string()));
    assert!(b.debug);
}
