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

#[cfg(all(test, target_arch = "wasm32"))]
mod auth_error_tests {
  // verifies: AUTH-026
  #[xmtp_common::test(unwrap_try = true)]
  fn auth_codes_reach_js_errors() {
    use xmtp_common::ErrorCode;
    use xmtp_proto::api::{ApiClientError, AuthError};
    for auth in [
      AuthError::CredentialRejected { retryable: true },
      AuthError::CallbackFailed { retryable: false },
      AuthError::Exhausted,
      AuthError::MissingCredential,
    ] {
      let api = xmtp_api::dyn_err(ApiClientError::from(auth));
      let error: wasm_bindgen::JsValue = super::ErrorWrapper::js(api).into();
      assert_eq!(
        js_sys::Reflect::get(&error, &"code".into())?.as_string()?,
        auth.error_code()
      );
      assert_eq!(
        js_sys::Reflect::get(&error, &"message".into())?.as_string()?,
        format!("[{}] {}", auth.error_code(), auth)
      );
    }
  }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod configuration_error_tests {
  // verifies: CONF-064
  #[xmtp_common::test(unwrap_try = true)]
  fn configuration_codes_reach_js_errors() {
    use std::collections::BTreeSet;
    use xmtp_common::ErrorCode;
    use xmtp_mls::client::ClientError;
    use xmtp_mls::server_configuration::ConfigurationFetchError;

    let errors = [
      ClientError::ConfigurationUnavailable(Box::new(ConfigurationFetchError::Api(
        xmtp_api::ApiError::EnvelopeTooLarge,
      ))),
      ClientError::ConfigurationInvalid(xmtp_configuration::ServerConfigurationError::Identifier),
      ClientError::BackendMismatch {
        stored: "org.xmtp.stored".to_string(),
        received: "org.xmtp.received".to_string(),
      },
      ClientError::ClientVersionTooOld {
        client: "1.0.0".to_string(),
        minimum: "2.0.0".to_string(),
      },
      ClientError::AuthRequired {
        required_scopes: vec!["messages:write".to_string()],
      },
      ClientError::ChainNotAccepted {
        chain: "eip155:1".to_string(),
        accepted: vec!["eip155:8453".to_string()],
      },
    ];

    let expected = [
      "ClientError::ConfigurationUnavailable",
      "ClientError::ConfigurationInvalid",
      "ClientError::BackendMismatch",
      "ClientError::ClientVersionTooOld",
      "ClientError::AuthRequired",
      "ClientError::ChainNotAccepted",
    ];

    let mut seen = BTreeSet::new();
    for (error, expected) in errors.iter().zip(expected) {
      assert_eq!(error.error_code(), expected);
      // A distinct type, not a message string: every code is its own.
      assert!(seen.insert(error.error_code()));
    }

    for error in errors {
      let code = error.error_code();
      let display = error.to_string();
      let js: wasm_bindgen::JsValue = super::ErrorWrapper::js(error).into();
      assert_eq!(
        js_sys::Reflect::get(&js, &"code".into())?.as_string()?,
        code
      );
      assert_eq!(
        js_sys::Reflect::get(&js, &"message".into())?.as_string()?,
        format!("[{code}] {display}")
      );
    }
  }
}
