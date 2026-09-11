pub use bindings_wasm_macros::wasm_bindgen_numbered_enum;
use serde_wasm_bindgen::Serializer;
use wasm_bindgen::{JsError, JsValue};
use xmtp_common::ErrorCode;

/// Wrapper for errors that implement ErrorCode trait.
/// Prefixes the error message with the error code.
///
/// Format: `[ErrorType::Variant] error message`
///
/// JavaScript usage:
/// ```js
/// try {
///   await client.doSomething();
/// } catch (e) {
///   console.log(e.message); // "[ErrorType::Variant] error message"
/// }
/// ```
#[derive(Debug)]
pub struct ErrorWrapper<E>(pub E)
where
  E: ErrorCode;

impl<T: ErrorCode> std::fmt::Display for ErrorWrapper<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    std::fmt::Display::fmt(&self.0, f)
  }
}

impl<T> From<T> for ErrorWrapper<T>
where
  T: ErrorCode,
{
  fn from(err: T) -> ErrorWrapper<T> {
    ErrorWrapper(err)
  }
}

impl<T: ErrorCode + 'static> ErrorWrapper<T> {
  /// Converts any error implementing `ErrorCode` into a `JsError` with
  /// the `[ErrorCode] message` format and a `.code` property.
  pub(crate) fn js(e: T) -> JsError {
    ErrorWrapper(e).into()
  }
}

impl<T: ErrorCode + 'static> From<ErrorWrapper<T>> for JsError {
  fn from(e: ErrorWrapper<T>) -> JsError {
    error_to_js(&e.0)
  }
}

/// Retains structured stream failures in the message across worker transfers.
pub(crate) fn error_to_js<T: ErrorCode + 'static>(error: &T) -> JsError {
  let code = error.error_code();
  let details =
    xmtp_mls::subscriptions::stream_failure::encode_stream_failure(error).unwrap_or_default();
  let js_error = JsError::new(&format!("[{}] {}{}", code, error, details));
  let js_value: JsValue = js_error.clone().into();
  let _ = js_sys::Reflect::set(
    &js_value,
    &JsValue::from_str("code"),
    &JsValue::from_str(code),
  );
  js_error
}

/// Converts a Rust value into a [`JsValue`].
pub(crate) fn to_value<T: serde::ser::Serialize + ?Sized>(
  value: &T,
) -> Result<JsValue, serde_wasm_bindgen::Error> {
  value.serialize(&Serializer::new().serialize_large_number_types_as_bigints(true))
}
