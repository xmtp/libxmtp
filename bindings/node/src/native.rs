use napi::bindgen_prelude::Error;
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

impl<T: ErrorCode> From<ErrorWrapper<T>> for napi::bindgen_prelude::Error {
  fn from(e: ErrorWrapper<T>) -> napi::bindgen_prelude::Error {
    let code = e.0.error_code();
    Error::from_reason(format!("[{}] {}", code, e.0))
  }
}
