extern crate proc_macro;

mod async_trait;
mod builder;
mod builders;
mod error_code;
mod log_macros;
mod logging;
mod parse_logs_macro;
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
