extern crate proc_macro;

mod async_trait;
mod error_code;
mod log_macros;
mod logging;
mod sdk_export;
mod sdk_member;
mod span_macro;
mod test_macro;
mod timeout_macro;

#[cfg(test)]
mod facade_markers_test;
#[cfg(test)]
mod sdk_export_test;
#[cfg(test)]
mod sdk_member_test;
#[cfg(test)]
mod timeout_macro_test;

/// Export an impl block, trait, function, record, or enum through UniFFI on
/// native and wasm32 targets, and describe it to the SDK generator.
///
/// If the item has an async function, native targets use the Tokio async
/// runtime. Sync-only items use plain `uniffi::export` on every target.
/// Every method in an impl block and every free function gets a tracing span.
/// Trait methods with a default body also get a span. Functions that return
/// `Result` record errors. An existing `#[tracing::instrument]` is kept.
/// On a record or enum, make this the first attribute, above every derive: a
/// derive written above it expands first, and the macro never sees it. The
/// `#[derive(uniffi::Record)]` or `#[derive(uniffi::Enum)]` below it still
/// exports the type.
///
/// Arguments:
///
/// - `native_only` / `wasm_only`: limit the whole item to one target, as
///   `#[cfg(not(target_arch = "wasm32"))]` or `#[cfg(target_arch = "wasm32")]`
///   above it would.
/// - `pure`: a synchronous free function with value-only arguments, for the
///   browser's main-thread module. It can forward stock argument defaults with
///   `pure, default(name = None)`. The SDK generator rejects object, client,
///   and foreign-trait arguments on a pure export.
/// - `client_static`: an asynchronous free function that every SDK also
///   exposes as a static member of its Client. The static's name is the
///   function's without a trailing `_with_backend`, in the SDK's casing, and
///   it takes the function's arguments in order with the `BackendSource`
///   argument moved last: `can_message_with_backend(backend, identities)`
///   becomes `Client.canMessage(identities, backend)`. The function stays
///   exported as well. It cannot be `native_only` or `wasm_only`. The
///   `BackendSource` cannot have a default, and no defaulted parameter can
///   end the function after it: the static takes the backend last, so a
///   TypeScript caller could not leave that parameter out.
///
/// Members take `#[sdk(...)]`:
///
/// - `#[sdk(immutable)]` on a synchronous `&self` method without arguments
///   that returns a value: the value never changes for the object's lifetime.
///   The SDKs expose it as a readonly property, and the browser bridge reads
///   it once. Such a method needs the option unless the bridge never forwards
///   it: its item is `native_only`, or its doc comment says `@xmtp-worker`.
///   Make a live read async instead.
/// - `#[sdk(kind = "namespace.name")]` on an enum variant: the public string
///   of the variant. `EventKind` takes one per variant. Mark every variant of
///   the enum or none.
/// - `#[sdk(redact)]` on a named record or variant field: generated Kotlin
///   `toString` and Swift `description` print `<redacted>` for it.
///   `#[sdk(redact = "key")]` hides one key of a string map field; the key
///   holds ASCII letters, digits, `_`, `.`, and `-`. Redaction fails closed:
///   every other field of that record, or of any variant of that enum, takes
///   `#[sdk(redact)]` or `#[sdk(shown)]`, and the macro implements `Debug`
///   for the type by calling its `fn redacted_debug(&self, f: &mut
///   Formatter<'_>) -> fmt::Result`, which the type writes. A derived `Debug` then conflicts
///   with it, wherever the derive sits. A `uniffi::Error` type cannot redact
///   a field: the Kotlin binding renames it.
///
/// `pure`, `client_static`, and each member option but `shown` become a
/// `#[doc = "@xmtp-..."]` line that UniFFI carries into the library metadata,
/// and a type with a redacted field gets `@xmtp-redacted`. The generator reads them and strips
/// them from generated documentation; see `apps/xmtp_sdk_bindgen/README.md`.
/// The macro rejects these markers in a doc comment, where they would skip
/// its checks. When rustc reports "cannot find
/// attribute `sdk` in this scope", the item lacks `#[xmtp_macro::sdk_export]`
/// as its first attribute.
/// The caller must depend on `uniffi` and `tracing`.
///
/// ```ignore
/// #[xmtp_macro::sdk_export]
/// impl Client {
///     #[sdk(immutable)]
///     pub fn inbox_id(&self) -> InboxId { /* ... */ }
///     pub async fn sync(&self) -> Result<(), SyncError> { /* ... */ }
/// }
///
/// #[xmtp_macro::sdk_export]
/// #[derive(Clone, Copy, uniffi::Enum)]
/// pub enum EventKind {
///     #[sdk(kind = "conversation.joined")]
///     ConversationJoined,
/// }
///
/// #[xmtp_macro::sdk_export]
/// #[derive(Clone, uniffi::Record)]
/// pub struct Credential {
///     #[sdk(shown)]
///     pub name: Option<String>,
///     #[sdk(redact)]
///     pub value: String,
/// }
///
/// impl Credential {
///     fn redacted_debug(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
///         f.debug_struct("Credential").field("name", &self.name).finish_non_exhaustive()
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn sdk_export(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    sdk_export::sdk_export(attr.into(), input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Check that a foreign-trait error type accepts unexpected UniFFI callback errors.
///
/// Put this attribute on a concrete error enum or struct. The caller must
/// depend on `uniffi`.
///
/// ```
/// #[xmtp_macro::callback_error]
/// #[derive(Debug, thiserror::Error)]
/// #[error("callback failed")]
/// struct CallbackError;
///
/// impl From<uniffi::UnexpectedUniFFICallbackError> for CallbackError {
///     fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
///         Self
///     }
/// }
/// ```
///
/// ```compile_fail
/// #[xmtp_macro::callback_error]
/// #[derive(Debug, thiserror::Error)]
/// #[error("callback failed")]
/// struct CallbackError;
/// ```
#[proc_macro_attribute]
pub fn callback_error(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    sdk_export::callback_error(attr.into(), input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

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
/// // → #[tracing::instrument(err, skip_all, fields(operation = "rpc.upload_key_package",
/// //        sentry.op = "rpc", sentry.name = "rpc.upload_key_package"))]
/// ```
#[proc_macro_attribute]
pub fn rpc_span(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    span_macro::rpc_span(attr, body)
}

/// Instrument an `xmtp_db` query method as `operation = "db.<fn_name>"` in
/// libxmtp's canonical, OTEL-safe span form (`err, skip_all`). Use
/// `#[db_span(redact_error)]` to log a fixed error field and return the original
/// error unchanged. Surfaces as `xmtp.db.*` Collector metrics. See [`span`] for the shared rationale.
#[proc_macro_attribute]
pub fn db_span(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    span_macro::db_span(attr, body)
}

/// Instrument a high-level MLS operation as `operation = "mls.<fn_name>"` in
/// libxmtp's canonical, OTEL-safe span form (`err, skip_all`). Use
/// `#[mls_span(redact_error)]` to log a fixed error field and return the original
/// error unchanged. Surfaces as `xmtp.mls.*` Collector metrics. See [`span`] for the shared rationale.
#[proc_macro_attribute]
pub fn mls_span(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    span_macro::mls_span(attr, body)
}

/// Instrument a method as a telemetry operation span in libxmtp's single
/// canonical, OTEL-safe form: `#[tracing::instrument(err, skip_all,
/// fields(operation = "<prefix>.<fn_name>", sentry.op = "<prefix>",
/// sentry.name = "<prefix>.<fn_name>"))]`.
///
/// `err` records span status=error on an `Err` return; `skip_all` keeps every
/// argument off the span so a per-call id can never leak in and explode
/// trace-attribute cardinality. `operation` is the single dimension the
/// Collector's `span_metrics` connector buckets on. Making this the only
/// writable form guarantees those invariants at compile time — no runtime test.
///
/// `sentry.op`/`sentry.name` are static vendor hints (same pattern as `otel.*`)
/// consumed by sentry-tracing; without them every Sentry span arrives as
/// op = "default".
///
/// This is the escape hatch for a namespace without a dedicated attribute;
/// prefer [`rpc_span`] / [`db_span`] / [`mls_span`] where they apply.
///
/// ```ignore
/// #[xmtp_macro::span(prefix = "stream")]
/// pub async fn subscribe(&self, ..) -> Result<..> { .. }
/// // → operation = "stream.subscribe", sentry.op = "stream",
/// //   sentry.name = "stream.subscribe"
/// ```
#[proc_macro_attribute]
pub fn span(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    span_macro::span(attr, body)
}

/// Error-case-only tracing for an FFI-exported fn: a `trace`-level `skip_all`
/// span carrying `sentry.op = "ffi"` / `sentry.name = "<fn_name>"`, plus an
/// ERROR event on an `Err` return.
///
/// The span is `trace`-level, so under normal filters the success path emits
/// nothing, and `skip_all` keeps arguments (keys, pins, paths) off the span.
/// `sentry.op`/`sentry.name` are static vendor hints, as in [`span`].
///
/// An async fn gets more than the attribute: its body is rewritten into a
/// nested `async move` bound to a per-call Sentry Hub (`bind_task_hub`), so
/// concurrent FFI calls keep separate breadcrumb trails, and the ERROR event is
/// emitted by hand *inside* that hub instead of by `instrument(err)` — `err`
/// fires only after the awaited body returns, by which point the task hub is
/// gone and the event Sentry promotes to an issue carries none of its
/// breadcrumbs. A sync fn keeps plain `instrument(.., err)` (rare at FFI).
///
/// Unlike [`span`], this is napi-safe: napi-rs clones every method attribute
/// onto the `extern "C"` wrapper it generates (which returns a raw
/// `napi_value`, not `Result`), so a bare `#[tracing::instrument(err)]` on an
/// exported method fails to compile. This macro detects the wrapper by its
/// `extern` ABI and passes it through untouched, instrumenting only the real
/// method.
///
/// ```ignore
/// #[napi]
/// #[xmtp_common::err_span]
/// pub async fn sync(&self) -> Result<()> { .. }
/// ```
#[proc_macro_attribute]
pub fn err_span(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    span_macro::err_span(attr, body)
}
