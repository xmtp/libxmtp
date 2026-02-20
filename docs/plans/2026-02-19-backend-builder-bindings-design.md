# Backend Builder for Node and WASM Bindings

**Date:** 2026-02-19
**Status:** Approved

## Problem

The `create_client` functions in Node and WASM bindings accept many raw API parameters (`v3_host`, `gateway_host`, `is_secure`, `auth_callback`, `auth_handle`, `app_version`) that could be encapsulated in a builder pattern. This would:

- Allow pre-constructing the backend separately from the client
- Auto-resolve API URLs from environment constants
- Derive `is_secure` from URL schemes (no manual flag)
- Validate configuration before building
- Reduce `create_client` parameter count

Mobile bindings already have this pattern via `connect_to_backend()`.

## Scope

- Node (`bindings/node`) and WASM (`bindings/wasm`) bindings only
- Generic builder macros reusable beyond BackendBuilder

## Design

### 1. Generic Builder Macros (`xmtp_macro`)

Two attribute macros that generate binding-annotated builder patterns for **any** struct:

#### `#[napi_builder]`

```rust
#[napi_builder]
pub struct FooBuilder {
    #[builder(required)]       // Passed in constructor
    name: String,

    #[builder(optional)]       // Setter generated, field is Option<T>
    description: Option<String>,

    #[builder(optional)]       // Works with Arc<T> and other complex types
    callback: Option<Arc<dyn SomeTrait>>,

    #[builder(default = "42")] // Has a default value
    count: u32,

    #[builder(skip)]           // No setter generated (internal use)
    internal: Vec<u8>,
}
```

**Generates:**

```rust
#[napi]
pub struct FooBuilder {
    name: String,
    description: Option<String>,
    callback: Option<Arc<dyn SomeTrait>>,
    count: u32,
    internal: Vec<u8>,
}

#[napi]
impl FooBuilder {
    #[napi(constructor)]
    pub fn new(name: String) -> Self {
        Self {
            name,
            description: None,
            callback: None,
            count: 42,
            internal: Vec::new(),
        }
    }

    #[napi]
    pub fn description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    #[napi]
    pub fn callback(mut self, callback: Arc<dyn SomeTrait>) -> Self {
        self.callback = Some(callback);
        self
    }

    #[napi]
    pub fn count(mut self, count: u32) -> Self {
        self.count = count;
        self
    }
}
```

#### `#[wasm_builder]`

Same field attributes, but generates `#[wasm_bindgen]` annotations with automatic `js_name = camelCase` conversion:

```rust
#[wasm_bindgen]
impl FooBuilder {
    #[wasm_bindgen(constructor)]
    pub fn new(name: String) -> Self { ... }

    #[wasm_bindgen(js_name = description)]
    pub fn description(mut self, description: String) -> Self { ... }

    #[wasm_bindgen(js_name = someField)]
    pub fn some_field(mut self, value: String) -> Self { ... }
}
```

#### Macro Behavior

- **Consuming self**: All setters take `mut self` and return `Self` to enable JS method chaining
- **`build()` not generated**: Left for manual implementation with domain-specific validation logic
- **Optional fields**: Setter takes the inner type `T`, wraps in `Some(T)` internally
- **Default fields**: Setter takes the type directly, replaces the default
- **Required fields**: Passed as constructor parameters, not generated as setters
- **Skip fields**: Must implement `Default` (initialized via `Default::default()`)
- **Complex types**: Must support `Arc<T>`, `Box<T>`, and other wrapper types for bindings-compatible complex types (e.g., `Arc<dyn AuthCallback>`)

#### Macro Test Suite

The macros must have an exhaustive test suite covering:

- **Primitive types**: `String`, `bool`, `u32`, `u64`, `i32`, `f64`
- **Complex types**: `Arc<dyn Trait>`, `Arc<ConcreteStruct>`, `Box<dyn Trait>`
- **Bindings-compatible complex types**: Ensure `Arc<BindingsCompatibleType>` setters work correctly with types that carry binding annotations (e.g., `#[napi]` structs, `#[wasm_bindgen]` classes)
- **Option wrapping**: `optional` fields correctly wrap values in `Some()`
- **Default values**: `default` fields initialize correctly and can be overridden
- **Required fields**: Passed in constructor, not available as setters
- **Skip fields**: Not exposed, initialized via `Default::default()`
- **Method chaining**: Consuming self pattern works across multiple setter calls
- **Multiple required fields**: Constructor accepts multiple parameters
- **Mixed field types**: Structs with a combination of required, optional, default, and skip fields
- **camelCase conversion** (WASM only): `snake_case` field names produce correct `js_name` attributes
- **Compilation failures**: Structs with invalid attribute combinations produce clear error messages

### 2. XmtpEnv Enum (`xmtp_configuration`)

```rust
pub enum XmtpEnv {
    // Centralized (V3) - auto-resolve api_url from NODE constants
    Local,
    Dev,
    Production,
    // Decentralized (D14n) - no api_url, gateway_host required from caller
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
        matches!(self, Self::TestnetStaging | Self::TestnetDev | Self::Testnet | Self::Mainnet)
    }
}
```

Each binding wraps this with its own annotations:

- Node: `#[napi(string_enum)]`
- WASM: `#[wasm_bindgen_numbered_enum]`

### 3. Shared Validation Logic

Located in `xmtp_api_d14n` or `xmtp_configuration`:

```rust
pub struct ResolvedBackendConfig {
    pub api_url: Option<String>,       // Set for centralized envs
    pub gateway_host: Option<String>,  // Set for d14n envs
    pub is_secure: bool,               // Derived from URL scheme(s)
    pub readonly: bool,
    pub app_version: String,
}

pub fn validate_and_resolve(
    env: XmtpEnv,
    api_url_override: Option<String>,
    gateway_host: Option<String>,
    readonly: bool,
    app_version: Option<String>,
    has_auth: bool,
) -> Result<ResolvedBackendConfig> {
    // Validation rules:
    // 1. env is required (enforced by type system)
    // 2. Centralized envs: resolve api_url from constants, apply override if provided
    // 3. D14n envs: gateway_host is required, api_url must NOT be set
    // 4. If auth_callback or auth_handle present: gateway_host is required
    // 5. is_secure derived from resolved URL(s): starts_with("https")
}
```

### 4. ClientBundleBuilder Changes (`xmtp_api_d14n`)

Make `v3_host` optional to support D14n-only mode:

```rust
// In client_bundle.rs, change __ClientBundleBuilder:
// Before: v3_host: String (always required)
// After:  v3_host: String (optional, only required when no gateway_host)

// build() logic:
// - If gateway_host + v3_host: D14n mode (current behavior)
// - If gateway_host only: D14n mode, no V3 fallback
// - If v3_host only: V3 mode (current behavior)
// - Neither: error
```

### 5. BackendBuilder in Bindings

#### Node (`bindings/node/src/client/backend.rs`)

```rust
#[napi_builder]
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

// Manual build() implementation
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
        )?;

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

        let bundle = builder.build()?;
        Ok(Backend(bundle))
    }
}

#[napi]
pub struct Backend(xmtp_mls::XmtpClientBundle);
```

#### WASM (`bindings/wasm/src/client/backend.rs`)

Same structure but using `#[wasm_builder]` and WASM-specific auth types.

### 6. Alternative `create_client`

Each binding gets a new function with backend-related params removed:

```rust
// Node
#[napi]
pub async fn create_client_with_backend(
    backend: &Backend,
    db: DbOptions,
    inbox_id: String,
    account_identifier: Identifier,
    device_sync_worker_mode: Option<SyncWorkerMode>,
    log_options: Option<LogOptions>,
    allow_offline: Option<bool>,
    nonce: Option<BigInt>,
    client_mode: Option<ClientMode>,
) -> Result<Client>
```

**Removed from signature:** `v3_host`, `gateway_host`, `is_secure`, `app_version`, `auth_callback`, `auth_handle`

Internally, this function:

1. Sets up logging
2. Creates database (persistent or ephemeral)
3. Creates cursor store from database
4. Wraps the pre-built backend bundle with cursor store via `MessageBackendBuilder::from_bundle()`
5. Builds the MLS client

The existing `create_client` functions remain unchanged for backwards compatibility.

### 7. JS Consumer Experience

```js
// Before: everything in one call
const client = await createClient(
  "https://grpc.dev.xmtp.network:443",
  null, true, dbOpts, inboxId, accountId,
  null, null, null, null, null, null, null, null
);

// After: builder pattern
const backend = await new BackendBuilder("dev")
  .appVersion("MyApp/1.0")
  .build();

const client = await createClientWithBackend(
  backend, dbOpts, inboxId, accountId
);

// D14n example
const d14nBackend = await new BackendBuilder("testnet")
  .gatewayHost("https://gateway.testnet.xmtp.network:443")
  .authCallback(myAuthCallback)
  .authHandle(myAuthHandle)
  .build();
```

## Layer Diagram

```
┌──────────────────────────────────────────────────────────┐
│                bindings/node & bindings/wasm              │
│  BackendBuilder (macro-generated setters + manual build) │
│  Backend (wraps ClientBundle)                            │
│  create_client_with_backend() (takes Backend)            │
└────────────────────────┬─────────────────────────────────┘
                         │
┌────────────────────────▼─────────────────────────────────┐
│  xmtp_configuration: XmtpEnv enum + URL constants        │
│  xmtp_api_d14n: ClientBundleBuilder (v3_host optional)   │
│  xmtp_api_d14n: validate_and_resolve() shared validation │
│  xmtp_macro: #[napi_builder] + #[wasm_builder] macros    │
└──────────────────────────────────────────────────────────┘
```
