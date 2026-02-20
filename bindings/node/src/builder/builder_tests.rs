/// Tests for the `#[napi_builder]` macro.
///
/// These tests verify that the macro correctly generates:
/// - A constructor (`new`) with all required fields as parameters
/// - `&mut self` setters for optional and default fields (exported to JS)
/// - Proper initialization of skip fields via `Default::default()`
///
/// Note: All struct fields exposed to JS must be NAPI-compatible types.
/// For complex Rust-only types (Arc<dyn Trait>, etc.), use `#[builder(skip)]`
/// and set them from Rust code after construction.
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    // -----------------------------------------------------------------------
    // Test 1: Primitive types (NAPI-compatible)
    // -----------------------------------------------------------------------
    #[xmtp_macro::napi_builder]
    pub struct PrimitiveBuilder {
        #[builder(required)]
        pub name: String,

        #[builder(optional)]
        pub flag: Option<bool>,

        #[builder(optional)]
        pub count: Option<u32>,

        #[builder(optional)]
        pub offset: Option<i32>,

        #[builder(optional)]
        pub ratio: Option<f64>,
    }

    #[test]
    fn test_primitive_required_only() {
        let b = PrimitiveBuilder::new("test".to_string());
        assert_eq!(b.name, "test");
        assert_eq!(b.flag, None);
        assert_eq!(b.count, None);
        assert_eq!(b.offset, None);
        assert_eq!(b.ratio, None);
    }

    #[test]
    fn test_primitive_all_set() {
        let mut b = PrimitiveBuilder::new("test".to_string());
        b.flag(true);
        b.count(42);
        b.offset(-5);
        b.ratio(3.14);
        assert_eq!(b.flag, Some(true));
        assert_eq!(b.count, Some(42));
        assert_eq!(b.offset, Some(-5));
        assert_eq!(b.ratio, Some(3.14));
    }

    // -----------------------------------------------------------------------
    // Test 2: Complex types via #[builder(skip)]
    // -----------------------------------------------------------------------
    // Arc<dyn Trait> is not NAPI-compatible, so use skip + Rust-side mutation.
    trait MyTrait: Send + Sync {
        fn value(&self) -> i32;
    }

    struct MyImpl(i32);
    impl MyTrait for MyImpl {
        fn value(&self) -> i32 {
            self.0
        }
    }

    #[xmtp_macro::napi_builder]
    pub struct ComplexBuilder {
        #[builder(required)]
        pub id: String,

        #[builder(skip)]
        callback: Option<Arc<dyn MyTrait>>,
    }

    #[test]
    fn test_arc_dyn_trait_via_skip() {
        let mut b = ComplexBuilder::new("id".to_string());
        assert!(b.callback.is_none());
        b.callback = Some(Arc::new(MyImpl(42)));
        assert_eq!(b.callback.unwrap().value(), 42);
    }

    // -----------------------------------------------------------------------
    // Test 3: Arc<ConcreteStruct> via skip
    // -----------------------------------------------------------------------
    struct ConcreteType {
        data: String,
    }

    #[xmtp_macro::napi_builder]
    pub struct ArcConcreteBuilder {
        #[builder(required)]
        pub id: String,

        #[builder(skip)]
        inner: Option<Arc<ConcreteType>>,
    }

    #[test]
    fn test_arc_concrete_via_skip() {
        let mut b = ArcConcreteBuilder::new("id".to_string());
        b.inner = Some(Arc::new(ConcreteType {
            data: "hello".to_string(),
        }));
        assert_eq!(b.inner.unwrap().data, "hello");
    }

    // -----------------------------------------------------------------------
    // Test 4: Default values
    // -----------------------------------------------------------------------
    #[xmtp_macro::napi_builder]
    pub struct DefaultBuilder {
        #[builder(required)]
        pub name: String,

        #[builder(default = "42")]
        pub count: u32,

        #[builder(default = "true")]
        pub enabled: bool,
    }

    #[test]
    fn test_defaults_applied() {
        let b = DefaultBuilder::new("test".to_string());
        assert_eq!(b.count, 42);
        assert!(b.enabled);
    }

    #[test]
    fn test_defaults_overridden() {
        let mut b = DefaultBuilder::new("test".to_string());
        b.count(99);
        b.enabled(false);
        assert_eq!(b.count, 99);
        assert!(!b.enabled);
    }

    // -----------------------------------------------------------------------
    // Test 5: Skip fields (initialized via Default::default())
    // -----------------------------------------------------------------------
    #[xmtp_macro::napi_builder]
    pub struct SkipBuilder {
        #[builder(required)]
        pub name: String,

        #[builder(skip)]
        internal: Vec<u8>,
    }

    #[test]
    fn test_skip_initialized_to_default() {
        let b = SkipBuilder::new("test".to_string());
        assert!(b.internal.is_empty());
    }

    // -----------------------------------------------------------------------
    // Test 6: Setter calls
    // -----------------------------------------------------------------------
    #[xmtp_macro::napi_builder]
    pub struct SetterBuilder {
        #[builder(required)]
        pub a: String,

        #[builder(optional)]
        pub b: Option<String>,

        #[builder(optional)]
        pub c: Option<u32>,

        #[builder(default = "false")]
        pub d: bool,
    }

    #[test]
    fn test_setter_calls() {
        let mut result = SetterBuilder::new("first".to_string());
        result.b("second".to_string());
        result.c(3);
        result.d(true);
        assert_eq!(result.a, "first");
        assert_eq!(result.b, Some("second".to_string()));
        assert_eq!(result.c, Some(3));
        assert!(result.d);
    }

    // -----------------------------------------------------------------------
    // Test 7: Multiple required fields
    // -----------------------------------------------------------------------
    #[xmtp_macro::napi_builder]
    pub struct MultiRequiredBuilder {
        #[builder(required)]
        pub host: String,

        #[builder(required)]
        pub port: u32,

        #[builder(optional)]
        pub label: Option<String>,
    }

    #[test]
    fn test_multiple_required() {
        let mut b = MultiRequiredBuilder::new("localhost".to_string(), 8080);
        b.label("test".to_string());
        assert_eq!(b.host, "localhost");
        assert_eq!(b.port, 8080);
        assert_eq!(b.label, Some("test".to_string()));
    }

    // -----------------------------------------------------------------------
    // Test 8: Mixed field types (required + optional + default + skip)
    // -----------------------------------------------------------------------
    #[xmtp_macro::napi_builder]
    pub struct MixedBuilder {
        #[builder(required)]
        pub name: String,

        #[builder(optional)]
        pub description: Option<String>,

        #[builder(default = "0")]
        pub count: u32,

        #[builder(skip)]
        cache: Vec<u8>,
    }

    #[test]
    fn test_mixed_fields() {
        let mut b = MixedBuilder::new("test".to_string());
        b.description("desc".to_string());
        b.count(5);
        assert_eq!(b.name, "test");
        assert_eq!(b.description, Some("desc".to_string()));
        assert_eq!(b.count, 5);
        assert!(b.cache.is_empty());
    }

    #[test]
    fn test_mixed_minimal() {
        let b = MixedBuilder::new("minimal".to_string());
        assert_eq!(b.name, "minimal");
        assert_eq!(b.description, None);
        assert_eq!(b.count, 0);
        assert!(b.cache.is_empty());
    }
}
