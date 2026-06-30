extern crate proc_macro;

mod async_trait;
mod builder;
mod builders;
mod error_code;
mod log_macros;
mod logging;
mod parse_logs_macro;
mod retryable;
mod span_macro;
mod test_macro;
mod timeout_macro;

#[cfg(test)]
mod builder_test;
#[cfg(test)]
mod timeout_macro_test;

/// A proc macro attribute that wraps the input in an `async_trait` implementation,
/// delegating to the appropriate `async_trait` implementation based on the target architecture.
///
/// On wasm32 architecture, it delegates to `async_trait::async_trait(?Send)`.
/// On all other architectures, it delegates to `async_trait::async_trait`.
#[proc_macro_attribute]
pub fn async_trait(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    async_trait::async_trait(attr, input)
}

/// Attribute macro that generates a NAPI-annotated builder pattern for a struct.
///
/// Each field must be annotated with one of:
/// - `#[builder(required)]` — passed in the constructor; no setter generated
/// - `#[builder(optional)]` — field type must be `Option<T>`; setter takes `T`, wraps in `Some`
/// - `#[builder(default = "expr")]` — has a default value; setter takes the full type
/// - `#[builder(skip)]` — no setter; initialized via `Default::default()`
///
/// The macro generates a `new()` constructor (with all required fields as parameters)
/// and fluent setters for optional/default fields. The `build()` method is NOT generated;
/// implement it manually.
///
/// # Example
///
/// ```ignore
/// #[napi_builder]
/// pub struct FooBuilder {
///     #[builder(required)]
///     name: String,
///     #[builder(optional)]
///     desc: Option<String>,
///     #[builder(default = "42")]
///     count: u32,
///     #[builder(skip)]
///     internal: Vec<u8>,
/// }
/// ```
#[proc_macro_attribute]
pub fn napi_builder(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    builders::napi_builder(attr, input)
}

/// Attribute macro that generates a wasm_bindgen-annotated builder pattern for a struct.
///
/// Behaves identically to [`napi_builder`] but emits `#[wasm_bindgen]` annotations
/// instead of `#[napi]`, and generates `js_name = camelCase` attributes on setters.
///
/// See [`napi_builder`] for field attribute documentation.
#[proc_macro_attribute]
pub fn wasm_builder(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    builders::wasm_builder(attr, input)
}

/// Attribute macro that generates a UniFFI-annotated builder pattern for a struct.
///
/// Emits `#[derive(uniffi::Object)]` on the struct and `#[uniffi::export]` on the
/// impl block. UniFFI annotates the impl block as a whole rather than individual
/// methods, so `constructor_ann` and `setter_ann` are empty.
///
/// See [`napi_builder`] for field attribute documentation.
#[proc_macro_attribute]
pub fn uniffi_builder(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    builders::uniffi_builder(attr, input)
}

/// A test macro that delegates to the appropriate test framework based on the target architecture.
///
/// On wasm32 architecture, it delegates to `wasm_bindgen_test::wasm_bindgen_test`.
/// On all other architectures, it delegates to `tokio::test`.
///
/// When using with 'rstest', ensure any other test invocations come after rstest invocation.
/// # Example
///
/// ```ignore
/// #[test]
/// async fn test_something() {
///     assert_eq!(2 + 2, 4);
/// }
/// ```
#[proc_macro_attribute]
pub fn test(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    test_macro::test(attr, body)
}

/// Attribute macro that wraps a function to capture tracing logs and automatically
/// run the `log_parser` tool on them when the function completes.
///
/// This macro sets up a tracing subscriber that captures all log output to a buffer.
/// When the function returns (or panics), a drop guard writes the captured logs to
/// a temporary file and invokes `cargo run --release -p log_parser` to analyze them.
///
/// # Example
///
/// ```ignore
/// #[parser]
/// fn test_with_log_parsing() {
///     tracing::info!("This log will be captured and parsed");
///     // ... test code ...
/// }
/// ```
#[proc_macro_attribute]
pub fn parser(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    parse_logs_macro::parser(attr, body)
}

#[proc_macro_attribute]
pub fn build_logging_metadata(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    log_macros::build_logging_metadata(attr, item)
}

#[proc_macro]
pub fn log_event(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    log_macros::log_event(input)
}

/// Derive macro for the `ErrorCode` trait.
///
/// Automatically generates an `error_code()` implementation that returns
/// `"TypeName::VariantName"` for each enum variant, or `"TypeName"` for structs.
///
/// # Example
///
/// ```ignore
/// use xmtp_common::ErrorCode;
///
/// #[derive(Debug, thiserror::Error, ErrorCode)]
/// pub enum GroupError {
///     #[error("Group not found")]
///     NotFound,  // Returns "GroupError::NotFound"
///
///     #[error("Storage error: {0}")]
///     #[error_code(inherit)]  // Delegates to StorageError::error_code()
///     Storage(#[from] StorageError),
/// }
/// ```
///
/// # Attributes
///
/// - `#[error_code(inherit)]` - Delegate to the inner error's `error_code()` method.
///   Use this for single-field variants that wrap another error implementing `ErrorCode`.
///
/// - `#[error_code(remote = "path::Type")]` - Implement `ErrorCode` for a remote type.
///   The derived item should mirror the remote type's shape. Default codes use the derived
///   item's type name, so keep it aligned with the remote type's name unless overridden.
///
/// - `#[error_code("CustomCode")]` - Override the generated code with a custom value.
///   Use this to maintain backwards compatibility when renaming variants.
///
/// # Example: Custom Code for Backwards Compatibility
///
/// ```ignore
/// #[derive(Debug, thiserror::Error, ErrorCode)]
/// pub enum MyError {
///     // Renamed from "OldName" but keeps the old error code
///     #[error("new name")]
///     #[error_code("MyError::OldName")]
///     NewName,
/// }
/// ```
#[proc_macro_derive(ErrorCode, attributes(error_code))]
pub fn derive_error_code(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    error_code::derive_error_code(input)
}

/// Derive macro for the `xmtp_common::RetryableError` trait.
///
/// Generates an `is_retryable(&self) -> bool` implementation. By default every
/// variant is **not** retryable; annotate the ones that are. `#[from]` carries
/// **no** retry semantics — forwarding to a wrapped error is always explicit
/// via `#[retry(inherit)]`. (A workspace census found ~3× more `#[from]`
/// variants needing a hardcoded value than forwarding ones, so auto-forwarding
/// produced noise, not savings.)
///
/// # Example
///
/// ```ignore
/// use thiserror::Error;
/// use xmtp_common::Retryable;
///
/// #[derive(Debug, Error, Retryable)]
/// pub enum MyError {
///     // not retryable (the default) — including #[from] wrappers
///     #[error("bad input")]
///     BadInput,
///     #[error(transparent)]
///     Decode(#[from] prost::DecodeError),
///
///     // always retryable
///     #[error("server busy")]
///     #[retry]
///     ServerBusy,
///
///     // forwards to StorageError::is_retryable() — explicit
///     #[error(transparent)]
///     #[retry(inherit)]
///     Storage(#[from] StorageError),
///
///     // finer-grained: inspect the payload
///     #[error("generic: {0}")]
///     #[retry(when = this.contains("database is locked"))]
///     Generic(String),
/// }
/// ```
///
/// # Attributes
///
/// Container attribute (on the enum/struct):
///
/// - `#[retry(default = true)]` / `#[retry(default = false)]` — set the baseline
///   retryability for any variant with no rule of its own. Omitted ⇒ `false`.
///   On an *enum* container, `default` is the only valid key (anything else is a
///   compile error). A struct container additionally accepts exactly one of
///   `true`/`false`/`when`.
///
/// Generic types are supported — the impl carries the type's declared generics
/// and where-clause. Forwarding a generic field requires bounding it yourself
/// (e.g. `T: RetryableError`).
///
/// Variant attributes (first match wins, top to bottom):
///
/// - `#[retry(when = EXPR)]` — run an arbitrary `bool` expression with the
///   variant's fields in scope. A single tuple field binds to `this`; named
///   fields bind to their own names; multi-field tuples bind to `this0`, `this1`,
///   …  Only fields the expression references are bound (the rest pattern as
///   `..`/`_`), so unused fields never trip `-D warnings`. Bindings are
///   **shared references** (`&Field`), so `Copy`/numeric comparisons need an
///   explicit deref (e.g. `*latency > 500`). `EXPR` must be total — a
///   `panic!`/`unwrap` here would crash the retry path.
/// - `#[retry(false)]` — never retryable (e.g. an exception under `default = true`).
/// - `#[retry(true)]` or bare `#[retry]` — always retryable.
/// - `#[retry(inherit)]` — forward to the single inner field's `is_retryable()`.
///   The only way a variant forwards; `#[from]` alone does nothing.
/// - Anything else — falls back to the container `default` baseline.
///
/// On a struct, `#[derive(Retryable)]` returns the container baseline (or a
/// `#[retry(when = EXPR)]` expression evaluated against `self`).
#[proc_macro_derive(Retryable, attributes(retry))]
pub fn derive_retryable(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    retryable::derive_retryable(input)
}

/// Attribute macro that wraps an async test body with a WASM-compatible timeout.
///
/// This is a drop-in replacement for rstest's `#[timeout]` that works on
/// `wasm32-unknown-unknown` by using `xmtp_common::time::timeout` internally.
///
/// # Example
///
/// ```ignore
/// #[xmtp_common::test]
/// #[xmtp_common::timeout(std::time::Duration::from_secs(60))]
/// async fn test_something() { ... }
/// ```
#[proc_macro_attribute]
pub fn timeout(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    timeout_macro::timeout(attr, body)
}

/// Instrument an `ApiClientWrapper` RPC method as `operation = "rpc.<fn_name>"`
/// in libxmtp's canonical, OTEL-safe span form (`err, skip_all`). Surfaces as
/// `xmtp.api.*` Collector metrics. See [`span`] for the shared rationale.
///
/// ```ignore
/// #[xmtp_macro::rpc_span]
/// pub async fn upload_key_package(&self, ..) -> Result<()> { .. }
/// // → #[tracing::instrument(err, skip_all, fields(operation = "rpc.upload_key_package"))]
/// ```
#[proc_macro_attribute]
pub fn rpc_span(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    span_macro::rpc_span(attr, body)
}

/// Instrument an `xmtp_db` query method as `operation = "db.<fn_name>"` in
/// libxmtp's canonical, OTEL-safe span form (`err, skip_all`). Surfaces as
/// `xmtp.db.*` Collector metrics. See [`span`] for the shared rationale.
#[proc_macro_attribute]
pub fn db_span(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    span_macro::db_span(attr, body)
}

/// Instrument a high-level MLS operation as `operation = "mls.<fn_name>"` in
/// libxmtp's canonical, OTEL-safe span form (`err, skip_all`). Surfaces as
/// `xmtp.mls.*` Collector metrics. See [`span`] for the shared rationale.
#[proc_macro_attribute]
pub fn mls_span(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    span_macro::mls_span(attr, body)
}

/// Instrument a method as a telemetry operation span in libxmtp's single
/// canonical, OTEL-safe form: `#[tracing::instrument(err, skip_all,
/// fields(operation = "<prefix>.<fn_name>"))]`.
///
/// `err` records span status=error on an `Err` return; `skip_all` keeps every
/// argument off the span so a per-call id can never leak in and explode
/// trace-attribute cardinality. `operation` is the single dimension the
/// Collector's `span_metrics` connector buckets on. Making this the only
/// writable form guarantees those invariants at compile time — no runtime test.
///
/// This is the escape hatch for a namespace without a dedicated attribute;
/// prefer [`rpc_span`] / [`db_span`] / [`mls_span`] where they apply.
///
/// ```ignore
/// #[xmtp_macro::span(prefix = "stream")]
/// pub async fn subscribe(&self, ..) -> Result<..> { .. }
/// // → operation = "stream.subscribe"
/// ```
#[proc_macro_attribute]
pub fn span(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    span_macro::span(attr, body)
}
