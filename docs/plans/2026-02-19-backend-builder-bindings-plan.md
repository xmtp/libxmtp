# Backend Builder for Node and WASM Bindings - Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Expose a `BackendBuilder` struct in Node and WASM bindings that encapsulates API configuration, auto-resolves URLs from environment, validates options, and produces a `Backend` object for `create_client_with_backend`.

**Architecture:** Generic builder macros (`#[napi_builder]`, `#[wasm_builder]`) in `xmtp_macro` generate binding-annotated builder patterns. A shared `XmtpEnv` enum in `xmtp_configuration` handles URL resolution. `ClientBundleBuilder` is made v3_host-optional. Each binding gets a `BackendBuilder` using the macro and a `create_client_with_backend` function.

**Tech Stack:** Rust proc macros (syn/quote), napi-rs, wasm-bindgen, derive_builder

**Design Doc:** `docs/plans/2026-02-19-backend-builder-bindings-design.md`

---

### Task 1: XmtpEnv Enum in xmtp_configuration

**Files:**

- Create: `crates/xmtp_configuration/src/common/env.rs`
- Modify: `crates/xmtp_configuration/src/common.rs:1-11`

**Step 1: Write the failing test**

Add to `crates/xmtp_configuration/src/common/env.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_centralized_envs_have_api_url() {
        assert!(XmtpEnv::Local.default_api_url().is_some());
        assert!(XmtpEnv::Dev.default_api_url().is_some());
        assert!(XmtpEnv::Production.default_api_url().is_some());
    }

    #[test]
    fn test_d14n_envs_have_no_api_url() {
        assert!(XmtpEnv::TestnetStaging.default_api_url().is_none());
        assert!(XmtpEnv::TestnetDev.default_api_url().is_none());
        assert!(XmtpEnv::Testnet.default_api_url().is_none());
        assert!(XmtpEnv::Mainnet.default_api_url().is_none());
    }

    #[test]
    fn test_is_d14n() {
        assert!(!XmtpEnv::Local.is_d14n());
        assert!(!XmtpEnv::Dev.is_d14n());
        assert!(!XmtpEnv::Production.is_d14n());
        assert!(XmtpEnv::TestnetStaging.is_d14n());
        assert!(XmtpEnv::TestnetDev.is_d14n());
        assert!(XmtpEnv::Testnet.is_d14n());
        assert!(XmtpEnv::Mainnet.is_d14n());
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p xmtp_configuration test_centralized -- --no-capture`
Expected: FAIL - module/type not found

**Step 3: Write the implementation**

Create `crates/xmtp_configuration/src/common/env.rs`:

```rust
use super::api::{GrpcUrlsDev, GrpcUrlsLocal, GrpcUrlsProduction};

/// XMTP network environment.
///
/// Centralized environments (Local, Dev, Production) automatically resolve
/// API URLs from built-in constants. Decentralized environments require
/// explicit gateway_host configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmtpEnv {
    // Centralized (V3)
    Local,
    Dev,
    Production,
    // Decentralized (D14n)
    TestnetStaging,
    TestnetDev,
    Testnet,
    Mainnet,
}

impl XmtpEnv {
    /// Returns the default api_url (NODE constant) for centralized environments.
    /// Returns None for d14n environments.
    pub fn default_api_url(&self) -> Option<&'static str> {
        match self {
            Self::Local => Some(GrpcUrlsLocal::NODE),
            Self::Dev => Some(GrpcUrlsDev::NODE),
            Self::Production => Some(GrpcUrlsProduction::NODE),
            _ => None,
        }
    }

    /// Whether this is a decentralized environment.
    pub fn is_d14n(&self) -> bool {
        matches!(
            self,
            Self::TestnetStaging | Self::TestnetDev | Self::Testnet | Self::Mainnet
        )
    }
}
```

Add to `crates/xmtp_configuration/src/common.rs`:

```rust
mod env;
pub use env::*;
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p xmtp_configuration`
Expected: PASS

**Step 5: Commit**

```
feat: add XmtpEnv enum with URL resolution
```

---

### Task 2: Shared Validation Logic

**Files:**

- Create: `crates/xmtp_configuration/src/common/backend_config.rs`
- Modify: `crates/xmtp_configuration/src/common.rs`
- Modify: `crates/xmtp_configuration/Cargo.toml` (add `thiserror` dependency)

**Step 1: Write the failing test**

Add to `crates/xmtp_configuration/src/common/backend_config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_centralized_env_resolves_api_url() {
        let config = validate_and_resolve(
            XmtpEnv::Dev, None, None, false, None, false,
        ).unwrap();
        assert!(config.api_url.is_some());
        assert!(config.gateway_host.is_none());
        assert!(config.is_secure); // Dev URL is https
    }

    #[test]
    fn test_centralized_env_with_override() {
        let config = validate_and_resolve(
            XmtpEnv::Dev,
            Some("http://custom:5556".to_string()),
            None, false, None, false,
        ).unwrap();
        assert_eq!(config.api_url.unwrap(), "http://custom:5556");
        assert!(!config.is_secure); // http, not https
    }

    #[test]
    fn test_d14n_env_requires_gateway_host() {
        let result = validate_and_resolve(
            XmtpEnv::TestnetStaging, None, None, false, None, false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_d14n_env_with_gateway_host() {
        let config = validate_and_resolve(
            XmtpEnv::Testnet,
            None,
            Some("https://gateway.testnet.xmtp.network:443".to_string()),
            false, None, false,
        ).unwrap();
        assert!(config.api_url.is_none());
        assert!(config.gateway_host.is_some());
        assert!(config.is_secure);
    }

    #[test]
    fn test_auth_requires_gateway_host() {
        let result = validate_and_resolve(
            XmtpEnv::Dev, None, None, false, None, true,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_auth_with_gateway_host_succeeds() {
        let config = validate_and_resolve(
            XmtpEnv::Dev,
            None,
            Some("https://gateway.dev.xmtp.network:443".to_string()),
            false, None, true,
        ).unwrap();
        assert!(config.api_url.is_some());
        assert!(config.gateway_host.is_some());
    }

    #[test]
    fn test_is_secure_derived_from_urls() {
        let config = validate_and_resolve(
            XmtpEnv::Local, None, None, false, None, false,
        ).unwrap();
        assert!(!config.is_secure); // Local URLs are http
    }

    #[test]
    fn test_readonly_passthrough() {
        let config = validate_and_resolve(
            XmtpEnv::Dev, None, None, true, None, false,
        ).unwrap();
        assert!(config.readonly);
    }

    #[test]
    fn test_app_version_default() {
        let config = validate_and_resolve(
            XmtpEnv::Dev, None, None, false, None, false,
        ).unwrap();
        assert_eq!(config.app_version, "");
    }

    #[test]
    fn test_app_version_passthrough() {
        let config = validate_and_resolve(
            XmtpEnv::Dev, None, None, false, Some("MyApp/1.0".to_string()), false,
        ).unwrap();
        assert_eq!(config.app_version, "MyApp/1.0");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p xmtp_configuration test_centralized_env_resolves`
Expected: FAIL

**Step 3: Write the implementation**

Create `crates/xmtp_configuration/src/common/backend_config.rs`:

```rust
use super::env::XmtpEnv;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ResolvedBackendConfig {
    pub api_url: Option<String>,
    pub gateway_host: Option<String>,
    pub is_secure: bool,
    pub readonly: bool,
    pub app_version: String,
}

#[derive(Error, Debug)]
pub enum BackendConfigError {
    #[error("D14n environment '{0:?}' requires gateway_host to be set")]
    MissingGatewayHost(XmtpEnv),
    #[error("Authentication (auth_callback or auth_handle) requires gateway_host to be set")]
    AuthRequiresGateway,
}

fn is_url_secure(url: &str) -> bool {
    url.starts_with("https")
}

pub fn validate_and_resolve(
    env: XmtpEnv,
    api_url_override: Option<String>,
    gateway_host: Option<String>,
    readonly: bool,
    app_version: Option<String>,
    has_auth: bool,
) -> Result<ResolvedBackendConfig, BackendConfigError> {
    // Auth requires gateway_host
    if has_auth && gateway_host.is_none() {
        return Err(BackendConfigError::AuthRequiresGateway);
    }

    // D14n envs require gateway_host
    if env.is_d14n() && gateway_host.is_none() {
        return Err(BackendConfigError::MissingGatewayHost(env));
    }

    // Resolve api_url: override takes precedence, then constant for centralized envs
    let api_url = api_url_override.or_else(|| env.default_api_url().map(String::from));

    // Derive is_secure from resolved URLs
    let is_secure = match (&api_url, &gateway_host) {
        (Some(url), Some(gw)) => is_url_secure(url) && is_url_secure(gw),
        (Some(url), None) => is_url_secure(url),
        (None, Some(gw)) => is_url_secure(gw),
        (None, None) => false,
    };

    Ok(ResolvedBackendConfig {
        api_url,
        gateway_host,
        is_secure,
        readonly,
        app_version: app_version.unwrap_or_default(),
    })
}
```

Add to `crates/xmtp_configuration/src/common.rs`:

```rust
mod backend_config;
pub use backend_config::*;
```

Add `thiserror` to `crates/xmtp_configuration/Cargo.toml` dependencies.

**Step 4: Run tests to verify they pass**

Run: `cargo test -p xmtp_configuration`
Expected: PASS

**Step 5: Commit**

```
feat: add validate_and_resolve for backend configuration
```

---

### Task 3: Make v3_host Optional in ClientBundleBuilder

**Files:**

- Modify: `crates/xmtp_api_d14n/src/queries/client_bundle.rs:165-237`
- Modify: `crates/xmtp_api_d14n/src/queries/builder.rs:30-31`

**Step 1: Write the failing test**

Add a test in `crates/xmtp_api_d14n/src/queries/client_bundle.rs` (or a test module):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_d14n_without_v3_host() {
        // Should succeed: gateway_host is set, v3_host is not
        let mut builder = ClientBundleBuilder::default();
        builder
            .gateway_host("http://localhost:5052")
            .is_secure(false);
        let result = builder.build();
        assert!(result.is_ok());
        assert!(matches!(result.unwrap().kind(), ClientKind::D14n));
    }

    #[test]
    fn test_neither_host_fails() {
        // Should fail: neither v3_host nor gateway_host
        let mut builder = ClientBundleBuilder::default();
        builder.is_secure(false);
        let result = builder.build();
        assert!(result.is_err());
    }

    #[test]
    fn test_v3_only_still_works() {
        // Should succeed: existing behavior unchanged
        let mut builder = ClientBundleBuilder::default();
        builder.v3_host("http://localhost:5556").is_secure(false);
        let result = builder.build();
        assert!(result.is_ok());
        assert!(matches!(result.unwrap().kind(), ClientKind::V3));
    }
}
```

**Step 2: Run test to verify failure**

Run: `cargo test -p xmtp_api_d14n test_d14n_without_v3_host`
Expected: FAIL - `MissingV3Host` error because v3_host is currently required

**Step 3: Modify the build() method**

In `crates/xmtp_api_d14n/src/queries/client_bundle.rs`, change the build method at line 165:

Replace the `v3_host` requirement (line 175):

```rust
// Before:
let v3_host = v3_host.ok_or(MessageBackendBuilderError::MissingV3Host)?;
```

With branching based on what hosts are available:

```rust
let is_secure = is_secure.unwrap_or_default();
let readonly = readonly.unwrap_or_default();

match (v3_host, gateway_host) {
    (_, Some(gateway)) => {
        // D14n mode - gateway_host present (with or without v3_host)
        // ... existing D14n build logic using `gateway` ...
    }
    (Some(v3_host), None) => {
        // V3 mode - only v3_host
        // ... existing V3 build logic ...
    }
    (None, None) => {
        Err(MessageBackendBuilderError::MissingHost)
    }
}
```

Add a new error variant in `crates/xmtp_api_d14n/src/queries/builder.rs`:

```rust
#[error("Either v3_host or gateway_host must be provided")]
MissingHost,
```

Rename `MissingV3Host` to `MissingHost` (or keep both for backwards compat).

**Step 4: Run tests to verify they pass**

Run: `cargo test -p xmtp_api_d14n`
Expected: PASS

**Step 5: Run full test suite to ensure no regressions**

Run: `cargo test -p xmtp_mls`
Expected: PASS (existing behavior unchanged for callers that provide v3_host)

**Step 6: Commit**

```
feat: make v3_host optional in ClientBundleBuilder for D14n-only mode
```

---

### Task 4: Generic `#[napi_builder]` Macro

**Files:**

- Modify: `crates/xmtp_macro/src/lib.rs` (add new attribute macro)
- Create: `crates/xmtp_macro/src/builder.rs` (builder macro implementation)

**Step 1: Create the builder module with parsing logic**

Create `crates/xmtp_macro/src/builder.rs` with the core logic shared between napi and wasm:

```rust
use proc_macro2::TokenStream;
use quote::{quote, format_ident};
use syn::{ItemStruct, Field, Ident, Type, Visibility};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldMode {
    Required,
    Optional,
    Default,
    Skip,
}

pub struct BuilderField {
    pub ident: Ident,
    pub ty: Type,
    pub mode: FieldMode,
    pub default_value: Option<syn::Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BindingKind {
    Napi,
    Wasm,
}

// Parse #[builder(...)] attributes from a field
pub fn parse_field_attr(field: &Field) -> (FieldMode, Option<syn::Expr>) { ... }

// Convert snake_case to camelCase for WASM js_name
pub fn to_camel_case(s: &str) -> String { ... }

// Generate the struct definition with binding annotations
pub fn generate_struct(input: &ItemStruct, kind: BindingKind) -> TokenStream { ... }

// Generate the constructor
pub fn generate_constructor(
    fields: &[BuilderField],
    kind: BindingKind,
) -> TokenStream { ... }

// Generate setter methods
pub fn generate_setters(
    fields: &[BuilderField],
    kind: BindingKind,
) -> TokenStream { ... }
```

**Step 2: Wire up the attribute macros in lib.rs**

Add to `crates/xmtp_macro/src/lib.rs`:

```rust
mod builder;

/// Attribute macro that generates a NAPI-annotated builder pattern.
///
/// Fields are annotated with `#[builder(required)]`, `#[builder(optional)]`,
/// `#[builder(default = "expr")]`, or `#[builder(skip)]`.
///
/// Generates: constructor with required fields, chainable setters for optional/default fields.
/// Does NOT generate `build()` - implement that manually.
#[proc_macro_attribute]
pub fn napi_builder(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::ItemStruct);
    builder::expand_builder(input, builder::BindingKind::Napi)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Attribute macro that generates a wasm_bindgen-annotated builder pattern.
///
/// Same field attributes as `#[napi_builder]`. Additionally generates
/// `#[wasm_bindgen(js_name = camelCase)]` attributes on setter methods.
#[proc_macro_attribute]
pub fn wasm_builder(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::ItemStruct);
    builder::expand_builder(input, builder::BindingKind::Wasm)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
```

**Step 3: Implement `expand_builder` in builder.rs**

The full implementation should:

1. Parse each field's `#[builder(...)]` attribute
2. Separate fields into required, optional, default, skip categories
3. Generate struct with appropriate binding annotation (`#[napi]` or `#[wasm_bindgen]`)
4. Generate constructor with required fields
5. Generate chainable setters (consuming `self`, returning `Self`) for optional and default fields
6. For optional fields: setter takes inner type `T`, stores as `Some(T)`
7. For WASM: add `#[wasm_bindgen(js_name = camelCase)]` to each method
8. Skip fields are initialized with `Default::default()` in constructor

Example generated output for NAPI:

```rust
#[::napi_derive::napi]
pub struct FooBuilder {
    name: String,
    description: Option<String>,
    count: u32,
    internal: Vec<u8>,
}

#[::napi_derive::napi]
impl FooBuilder {
    #[::napi_derive::napi(constructor)]
    pub fn new(name: String) -> Self {
        Self {
            name,
            description: None,
            count: 42,
            internal: Default::default(),
        }
    }

    #[::napi_derive::napi]
    pub fn description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    #[::napi_derive::napi]
    pub fn count(mut self, count: u32) -> Self {
        self.count = count;
        self
    }
}
```

Example generated output for WASM:

```rust
#[::wasm_bindgen::prelude::wasm_bindgen]
pub struct FooBuilder {
    name: String,
    description: Option<String>,
    count: u32,
    internal: Vec<u8>,
}

#[::wasm_bindgen::prelude::wasm_bindgen]
impl FooBuilder {
    #[::wasm_bindgen::prelude::wasm_bindgen(constructor)]
    pub fn new(name: String) -> Self {
        Self {
            name,
            description: None,
            count: 42,
            internal: Default::default(),
        }
    }

    #[::wasm_bindgen::prelude::wasm_bindgen(js_name = description)]
    pub fn description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    #[::wasm_bindgen::prelude::wasm_bindgen(js_name = count)]
    pub fn count(mut self, count: u32) -> Self {
        self.count = count;
        self
    }
}
```

**Key implementation details for optional field setters:**

For a field declared as `description: Option<String>` with `#[builder(optional)]`:

- The setter parameter type is the INNER type (`String`), not `Option<String>`
- The setter wraps in `Some()`: `self.description = Some(description);`
- This means the macro must strip `Option<>` from the field type to get the setter parameter type

For a field like `callback: Option<Arc<dyn SomeTrait>>` with `#[builder(optional)]`:

- The setter parameter type is `Arc<dyn SomeTrait>`
- The setter wraps in `Some()`: `self.callback = Some(callback);`

**Step 4: Run the macro on a test struct**

Since proc macros can't be tested in the same crate, test in the Node binding (or create a small test fixture). Initial smoke test: just verify it compiles.

Run: `cargo check -p xmtp_macro`
Expected: PASS (the macro itself compiles)

**Step 5: Commit**

```
feat: add #[napi_builder] and #[wasm_builder] attribute macros
```

---

### Task 5: Macro Test Suite

**Files:**

- Create: `bindings/node/src/client/builder_tests.rs` (NAPI macro tests)
- Modify: integration tests or test modules in binding crates

Since proc macros must be tested in consumer crates, the exhaustive test suite lives in the binding crates. The tests verify compilation and behavior of macro-generated code.

**Step 1: Write NAPI builder macro tests**

These tests go in the Node binding crate where `napi` is available. Create test structs that exercise all field attribute combinations.

```rust
// bindings/node/src/client/builder_tests.rs
#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use napi_derive::napi;

    // Test 1: Primitive types (String, bool, u32, u64, i32, f64)
    #[xmtp_macro::napi_builder]
    pub struct PrimitiveBuilder {
        #[builder(required)]
        name: String,

        #[builder(optional)]
        flag: Option<bool>,

        #[builder(optional)]
        count: Option<u32>,

        #[builder(optional)]
        big_count: Option<u64>,

        #[builder(optional)]
        offset: Option<i32>,

        #[builder(optional)]
        ratio: Option<f64>,
    }

    #[test]
    fn test_primitive_types() {
        let b = PrimitiveBuilder::new("test".to_string())
            .flag(true)
            .count(42)
            .big_count(999999)
            .offset(-5)
            .ratio(3.14);
        assert_eq!(b.name, "test");
        assert_eq!(b.flag, Some(true));
        assert_eq!(b.count, Some(42));
        assert_eq!(b.big_count, Some(999999));
        assert_eq!(b.offset, Some(-5));
        assert_eq!(b.ratio, Some(3.14));
    }

    // Test 2: Complex types - Arc<dyn Trait>
    trait MyTrait: Send + Sync {
        fn value(&self) -> i32;
    }

    struct MyImpl(i32);
    impl MyTrait for MyImpl {
        fn value(&self) -> i32 { self.0 }
    }

    #[xmtp_macro::napi_builder]
    pub struct ComplexBuilder {
        #[builder(required)]
        id: String,

        #[builder(optional)]
        callback: Option<Arc<dyn MyTrait>>,
    }

    #[test]
    fn test_arc_dyn_trait() {
        let cb: Arc<dyn MyTrait> = Arc::new(MyImpl(42));
        let b = ComplexBuilder::new("id".to_string())
            .callback(cb);
        assert_eq!(b.callback.unwrap().value(), 42);
    }

    // Test 3: Arc<ConcreteStruct>
    struct ConcreteType { data: String }

    #[xmtp_macro::napi_builder]
    pub struct ArcConcreteBuilder {
        #[builder(required)]
        id: String,

        #[builder(optional)]
        inner: Option<Arc<ConcreteType>>,
    }

    #[test]
    fn test_arc_concrete() {
        let inner = Arc::new(ConcreteType { data: "hello".to_string() });
        let b = ArcConcreteBuilder::new("id".to_string())
            .inner(inner);
        assert_eq!(b.inner.unwrap().data, "hello");
    }

    // Test 4: Default values
    #[xmtp_macro::napi_builder]
    pub struct DefaultBuilder {
        #[builder(required)]
        name: String,

        #[builder(default = "42")]
        count: u32,

        #[builder(default = "true")]
        enabled: bool,
    }

    #[test]
    fn test_defaults() {
        let b = DefaultBuilder::new("test".to_string());
        assert_eq!(b.count, 42);
        assert!(b.enabled);
    }

    #[test]
    fn test_default_override() {
        let b = DefaultBuilder::new("test".to_string())
            .count(99)
            .enabled(false);
        assert_eq!(b.count, 99);
        assert!(!b.enabled);
    }

    // Test 5: Skip fields
    #[xmtp_macro::napi_builder]
    pub struct SkipBuilder {
        #[builder(required)]
        name: String,

        #[builder(skip)]
        internal: Vec<u8>,
    }

    #[test]
    fn test_skip_initialized_to_default() {
        let b = SkipBuilder::new("test".to_string());
        assert!(b.internal.is_empty());
    }

    // Test 6: Method chaining
    #[xmtp_macro::napi_builder]
    pub struct ChainBuilder {
        #[builder(required)]
        a: String,

        #[builder(optional)]
        b: Option<String>,

        #[builder(optional)]
        c: Option<u32>,

        #[builder(default = "false")]
        d: bool,
    }

    #[test]
    fn test_method_chaining() {
        let result = ChainBuilder::new("first".to_string())
            .b("second".to_string())
            .c(3)
            .d(true);
        assert_eq!(result.a, "first");
        assert_eq!(result.b, Some("second".to_string()));
        assert_eq!(result.c, Some(3));
        assert!(result.d);
    }

    // Test 7: Multiple required fields
    #[xmtp_macro::napi_builder]
    pub struct MultiRequiredBuilder {
        #[builder(required)]
        host: String,

        #[builder(required)]
        port: u32,

        #[builder(optional)]
        label: Option<String>,
    }

    #[test]
    fn test_multiple_required() {
        let b = MultiRequiredBuilder::new("localhost".to_string(), 8080)
            .label("test".to_string());
        assert_eq!(b.host, "localhost");
        assert_eq!(b.port, 8080);
        assert_eq!(b.label, Some("test".to_string()));
    }

    // Test 8: Mixed field types
    #[xmtp_macro::napi_builder]
    pub struct MixedBuilder {
        #[builder(required)]
        name: String,

        #[builder(optional)]
        description: Option<String>,

        #[builder(default = "0")]
        count: u32,

        #[builder(skip)]
        cache: Vec<u8>,

        #[builder(optional)]
        callback: Option<Arc<dyn MyTrait>>,
    }

    #[test]
    fn test_mixed_fields() {
        let b = MixedBuilder::new("test".to_string())
            .description("desc".to_string())
            .count(5);
        assert_eq!(b.name, "test");
        assert_eq!(b.description, Some("desc".to_string()));
        assert_eq!(b.count, 5);
        assert!(b.cache.is_empty());
        assert!(b.callback.is_none());
    }
}
```

**Step 2: Write WASM builder macro tests**

Similar tests in `bindings/wasm/src/` but verifying WASM-specific behavior (camelCase names verified through compilation, since wasm_bindgen would fail on incorrect js_name).

```rust
// Tests similar to above but using #[xmtp_macro::wasm_builder]
// Key additional test: snake_case fields produce camelCase js_name
#[xmtp_macro::wasm_builder]
pub struct CamelCaseBuilder {
    #[builder(required)]
    some_field: String,

    #[builder(optional)]
    another_long_name: Option<u32>,
}
// Verifies compilation generates:
// #[wasm_bindgen(js_name = someField)]
// #[wasm_bindgen(js_name = anotherLongName)]
```

**Step 3: Run tests**

Run: `cargo test -p bindings_node builder_tests`
Run: `cargo test -p bindings_wasm builder_tests` (or `dev/test/wasm`)
Expected: PASS

**Step 4: Commit**

```
test: add exhaustive test suite for napi_builder and wasm_builder macros
```

---

### Task 6: Node Binding - BackendBuilder and Backend

**Files:**

- Create: `bindings/node/src/client/backend.rs`
- Modify: `bindings/node/src/client/mod.rs` (add `mod backend;` and `pub use backend::*;`)
- Modify: `bindings/node/src/client/options.rs` (add XmtpEnv enum)

**Step 1: Add XmtpEnv wrapper enum to Node bindings**

In `bindings/node/src/client/options.rs`, add:

```rust
#[napi(string_enum)]
#[derive(Debug, Clone, Copy)]
pub enum XmtpEnv {
    Local,
    Dev,
    Production,
    TestnetStaging,
    TestnetDev,
    Testnet,
    Mainnet,
}

impl From<XmtpEnv> for xmtp_configuration::XmtpEnv {
    fn from(env: XmtpEnv) -> Self {
        match env {
            XmtpEnv::Local => Self::Local,
            XmtpEnv::Dev => Self::Dev,
            XmtpEnv::Production => Self::Production,
            XmtpEnv::TestnetStaging => Self::TestnetStaging,
            XmtpEnv::TestnetDev => Self::TestnetDev,
            XmtpEnv::Testnet => Self::Testnet,
            XmtpEnv::Mainnet => Self::Mainnet,
        }
    }
}
```

**Step 2: Create BackendBuilder and Backend structs**

Create `bindings/node/src/client/backend.rs`:

```rust
use crate::ErrorWrapper;
use crate::client::gateway_auth::{AuthCallback, AuthHandle};
use crate::client::options::XmtpEnv;
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use std::sync::Arc;
use xmtp_api_d14n::ClientBundle;
use xmtp_configuration::validate_and_resolve;

#[xmtp_macro::napi_builder]
pub struct BackendBuilder {
    #[builder(required)]
    env: XmtpEnv,

    #[builder(optional)]
    api_url: Option<String>,

    #[builder(optional)]
    gateway_host: Option<String>,

    #[builder(optional)]
    readonly: Option<bool>,

    #[builder(optional)]
    app_version: Option<String>,

    #[builder(optional)]
    auth_callback: Option<AuthCallback>,

    #[builder(optional)]
    auth_handle: Option<AuthHandle>,
}

#[napi]
impl BackendBuilder {
    #[napi]
    pub async fn build(self) -> Result<Backend> {
        let config = validate_and_resolve(
            self.env.into(),
            self.api_url,
            self.gateway_host,
            self.readonly.unwrap_or(false),
            self.app_version,
            self.auth_callback.is_some() || self.auth_handle.is_some(),
        )
        .map_err(ErrorWrapper::from)?;

        let mut builder = ClientBundle::builder();
        if let Some(url) = &config.api_url {
            builder.v3_host(url);
        }
        if let Some(host) = &config.gateway_host {
            builder.gateway_host(host);
        }
        builder
            .is_secure(config.is_secure)
            .readonly(config.readonly)
            .app_version(config.app_version)
            .maybe_auth_callback(self.auth_callback.map(|c| Arc::new(c) as _))
            .maybe_auth_handle(self.auth_handle.map(|h| h.into()));

        let bundle = builder.build().map_err(ErrorWrapper::from)?;
        Ok(Backend { bundle })
    }
}

#[napi]
#[derive(Clone)]
pub struct Backend {
    pub(crate) bundle: xmtp_mls::XmtpClientBundle,
}
```

**Step 3: Register the module**

Add to `bindings/node/src/client/mod.rs`:

```rust
pub mod backend;
```

**Step 4: Verify compilation**

Run: `cargo check -p bindings_node`
Expected: PASS

**Step 5: Commit**

```
feat(node): add BackendBuilder with environment-based URL resolution
```

---

### Task 7: Node Binding - create_client_with_backend

**Files:**

- Modify: `bindings/node/src/client/create_client.rs`

**Step 1: Add the new function**

Add to `bindings/node/src/client/create_client.rs`, after the existing `create_client` function:

```rust
use crate::client::backend::Backend;

#[napi]
pub async fn create_client_with_backend(
    backend: &Backend,
    db: DbOptions,
    inbox_id: String,
    account_identifier: Identifier,
    device_sync_worker_mode: Option<SyncWorkerMode>,
    log_options: Option<LogOptions>,
    allow_offline: Option<bool>,
    app_version: Option<String>,
    nonce: Option<BigInt>,
    client_mode: Option<ClientMode>,
) -> Result<Client> {
    let root_identifier = account_identifier.clone();
    init_logging(log_options.unwrap_or_default())?;

    let DbOptions {
        db_path,
        encryption_key,
        max_db_pool_size,
        min_db_pool_size,
    } = db;

    let db = if let Some(path) = db_path {
        NativeDb::builder().persistent(path)
    } else {
        NativeDb::builder().ephemeral()
    };

    let db = if let Some(max_size) = max_db_pool_size {
        db.max_pool_size(max_size)
    } else {
        db.max_pool_size(MAX_DB_POOL_SIZE)
    };

    let db = if let Some(min_size) = min_db_pool_size {
        db.min_pool_size(min_size)
    } else {
        db.min_pool_size(MIN_DB_POOL_SIZE)
    };

    let db = if let Some(key) = encryption_key {
        let key: Vec<u8> = key.deref().into();
        let key: EncryptionKey = key
            .try_into()
            .map_err(|_| Error::from_reason("Malformed 32 byte encryption key"))?;
        db.key(key).build()
    } else {
        db.build_unencrypted()
    }
    .map_err(ErrorWrapper::from)?;
    let store = EncryptedMessageStore::new(db).map_err(ErrorWrapper::from)?;

    let nonce = match nonce {
        Some(n) => {
            let (signed, value, lossless) = n.get_u64();
            if signed {
                return Err(Error::from_reason("`nonce` must be non-negative"));
            }
            if !lossless {
                return Err(Error::from_reason("`nonce` is too large"));
            }
            value
        }
        None => 1,
    };

    let internal_account_identifier = account_identifier.clone().try_into()?;
    let identity_strategy = IdentityStrategy::new(
        inbox_id.clone(),
        internal_account_identifier,
        nonce,
        None,
    );

    let cursor_store = SqliteCursorStore::new(store.db());
    let mut mbb = MessageBackendBuilder::default();
    mbb.cursor_store(cursor_store);
    let api_client = mbb.clone().from_bundle(backend.bundle.clone()).map_err(ErrorWrapper::from)?;
    let sync_api_client = mbb.from_bundle(backend.bundle.clone()).map_err(ErrorWrapper::from)?;

    let mut builder = xmtp_mls::Client::builder(identity_strategy)
        .api_clients(api_client, sync_api_client)
        .enable_api_stats()
        .map_err(ErrorWrapper::from)?
        .enable_api_debug_wrapper()
        .map_err(ErrorWrapper::from)?
        .with_remote_verifier()
        .map_err(ErrorWrapper::from)?
        .with_allow_offline(allow_offline)
        .store(store);

    if let Some(device_sync_worker_mode) = device_sync_worker_mode {
        builder = builder.device_sync_worker_mode(device_sync_worker_mode.into());
    };

    let xmtp_client = builder
        .default_mls_store()
        .map_err(ErrorWrapper::from)?
        .build()
        .await
        .map_err(ErrorWrapper::from)?;

    Ok(Client {
        inner_client: Arc::new(xmtp_client),
        account_identifier: root_identifier,
        app_version,
    })
}
```

**Step 2: Verify compilation**

Run: `cargo check -p bindings_node`
Expected: PASS

**Step 3: Run linting**

Run: `dev/lint`
Expected: PASS

**Step 4: Commit**

```
feat(node): add create_client_with_backend function
```

---

### Task 8: WASM Binding - BackendBuilder and Backend

**Files:**

- Create: `bindings/wasm/src/client/backend.rs`
- Modify: `bindings/wasm/src/client.rs` (add `mod backend;` and `pub use backend::*;`, add XmtpEnv enum)

**Step 1: Add XmtpEnv wrapper enum**

In `bindings/wasm/src/client.rs`, add:

```rust
#[wasm_bindgen_numbered_enum]
#[derive(Default)]
pub enum XmtpEnv {
    #[default]
    Local = 0,
    Dev = 1,
    Production = 2,
    TestnetStaging = 3,
    TestnetDev = 4,
    Testnet = 5,
    Mainnet = 6,
}

impl From<XmtpEnv> for xmtp_configuration::XmtpEnv {
    fn from(env: XmtpEnv) -> Self {
        match env {
            XmtpEnv::Local => Self::Local,
            XmtpEnv::Dev => Self::Dev,
            XmtpEnv::Production => Self::Production,
            XmtpEnv::TestnetStaging => Self::TestnetStaging,
            XmtpEnv::TestnetDev => Self::TestnetDev,
            XmtpEnv::Testnet => Self::Testnet,
            XmtpEnv::Mainnet => Self::Mainnet,
        }
    }
}
```

**Step 2: Create BackendBuilder and Backend**

Create `bindings/wasm/src/client/backend.rs` - similar to Node version but using `#[wasm_builder]`, WASM-specific auth types, and `JsError` instead of NAPI `Error`.

```rust
use crate::client::gateway_auth::{AuthCallback, AuthHandle};
use crate::client::XmtpEnv;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use xmtp_api_d14n::ClientBundle;
use xmtp_configuration::validate_and_resolve;

#[xmtp_macro::wasm_builder]
pub struct BackendBuilder {
    #[builder(required)]
    env: XmtpEnv,

    #[builder(optional)]
    api_url: Option<String>,

    #[builder(optional)]
    gateway_host: Option<String>,

    #[builder(optional)]
    readonly: Option<bool>,

    #[builder(optional)]
    app_version: Option<String>,

    #[builder(optional)]
    auth_callback: Option<AuthCallback>,

    #[builder(optional)]
    auth_handle: Option<AuthHandle>,
}

#[wasm_bindgen]
impl BackendBuilder {
    #[wasm_bindgen]
    pub async fn build(self) -> Result<Backend, JsError> {
        let config = validate_and_resolve(
            self.env.into(),
            self.api_url,
            self.gateway_host,
            self.readonly.unwrap_or(false),
            self.app_version,
            self.auth_callback.is_some() || self.auth_handle.is_some(),
        )
        .map_err(|e| JsError::new(&e.to_string()))?;

        let mut builder = ClientBundle::builder();
        if let Some(url) = &config.api_url {
            builder.v3_host(url);
        }
        if let Some(host) = &config.gateway_host {
            builder.gateway_host(host);
        }
        builder
            .is_secure(config.is_secure)
            .readonly(config.readonly)
            .app_version(config.app_version)
            .maybe_auth_callback(self.auth_callback.map(|c| Arc::new(c) as _))
            .maybe_auth_handle(self.auth_handle.map(|h| h.handle));

        let bundle = builder.build().map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Backend { bundle })
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct Backend {
    pub(crate) bundle: xmtp_mls::XmtpClientBundle,
}
```

**Step 3: Register the module**

Note: the `client` directory for WASM might not exist as a module directory yet. Check if `bindings/wasm/src/client.rs` needs to become `bindings/wasm/src/client/mod.rs` to add submodules, or if `backend.rs` should be placed at the `src/` level.

If `client.rs` is a single file, you need to convert it to `client/mod.rs` + `client/gateway_auth.rs` + `client/backend.rs`.

**Step 4: Verify compilation**

Run: `dev/check-wasm`
Expected: PASS

**Step 5: Commit**

```
feat(wasm): add BackendBuilder with environment-based URL resolution
```

---

### Task 9: WASM Binding - create_client_with_backend

**Files:**

- Modify: `bindings/wasm/src/client.rs` (or `client/mod.rs`)

**Step 1: Add the new function**

Similar to Node version but with WASM-specific types and error handling:

```rust
use crate::client::backend::Backend;

#[wasm_bindgen(js_name = createClientWithBackend)]
pub async fn create_client_with_backend(
    backend: &Backend,
    #[wasm_bindgen(js_name = inboxId)] inbox_id: String,
    #[wasm_bindgen(js_name = accountIdentifier)] account_identifier: Identifier,
    #[wasm_bindgen(js_name = dbPath)] db_path: Option<String>,
    #[wasm_bindgen(js_name = encryptionKey)] encryption_key: Option<Uint8Array>,
    #[wasm_bindgen(js_name = deviceSyncMode)] device_sync_worker_mode: Option<DeviceSyncMode>,
    #[wasm_bindgen(js_name = logOptions)] log_options: Option<LogOptions>,
    #[wasm_bindgen(js_name = allowOffline)] allow_offline: Option<bool>,
    #[wasm_bindgen(js_name = appVersion)] app_version: Option<String>,
    nonce: Option<u64>,
    #[wasm_bindgen(js_name = clientMode)] client_mode: Option<ClientMode>,
) -> Result<Client, JsError> {
    // Same pattern as existing create_client but uses backend.bundle
    // via MessageBackendBuilder::from_bundle() instead of building from scratch
    // ...
}
```

**Step 2: Verify compilation**

Run: `dev/check-wasm`
Expected: PASS

**Step 3: Commit**

```
feat(wasm): add createClientWithBackend function
```

---

### Task 10: Integration Verification

**Step 1: Run full linting**

Run: `dev/lint`
Expected: PASS

**Step 2: Run Node binding tests**

Run: `cd bindings/node && yarn && yarn build && yarn test`
Expected: PASS (existing tests still pass)

**Step 3: Run WASM binding check**

Run: `dev/check-wasm`
Expected: PASS

**Step 4: Run core crate tests**

Run: `cargo test -p xmtp_configuration -p xmtp_api_d14n`
Expected: PASS

**Step 5: Final commit and submit stack**

Run: `gt submit --no-interactive`

---

## Dependency Graph

```
Task 1 (XmtpEnv) ─────────────────┐
                                    ├─► Task 6 (Node BackendBuilder) ─► Task 7 (Node create_client_with_backend)
Task 2 (Validation) ───────────────┤
                                    ├─► Task 8 (WASM BackendBuilder) ─► Task 9 (WASM create_client_with_backend)
Task 3 (ClientBundleBuilder) ──────┘
                                                                        ↓
Task 4 (napi_builder macro) ───► Task 5 (Macro Tests) ──────────────► Task 10 (Integration)
Task 4 (wasm_builder macro) ───┘
```

Tasks 1, 2, 3, and 4 can be worked on in parallel. Tasks 6-9 depend on all four. Task 10 is final verification.
