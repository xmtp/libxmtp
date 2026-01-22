mod web;

use crate::client::LogLevel;
use crate::client::gateway_auth::{AuthCallback, AuthHandle};
use crate::client::{Client, LogOptions, create_client};
use crate::inbox_id::generate_inbox_id;
use alloy::signers::SignerSync;
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
use xmtp_configuration::GrpcUrls;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_id::InboxOwner;

/// Test that errors are formatted with the [ErrorCode] prefix
#[wasm_bindgen_test]
pub fn test_error_code_format() {
  use xmtp_cryptography::signature::IdentifierValidationError;

  // Create an error that implements ErrorCode
  let inner_error =
    IdentifierValidationError::InvalidAddresses(vec!["invalid-address".to_string()]);

  // Use our error function to format it
  let js_error = crate::error(inner_error);

  // Access the error message through JsValue
  let js_value: JsValue = js_error.into();
  let error_obj = js_sys::Error::from(js_value);
  let error_string = error_obj.message().as_string().unwrap_or_default();

  // Verify the error starts with [ErrorType::Variant] pattern
  assert!(
    error_string.starts_with("[IdentifierValidationError::InvalidAddresses]"),
    "Error should start with error code prefix, got: {}",
    error_string
  );
}

#[wasm_bindgen(js_name = createTestClient)]
pub async fn create_test_client(path: Option<String>) -> Client {
  // crate::opfs::Opfs::wipe_files().await.unwrap();
  let wallet = generate_local_wallet();
  let account_address = wallet.get_identifier().unwrap_throw();
  let host = GrpcUrls::NODE.to_string();
  let inbox_id = generate_inbox_id(account_address.clone().into(), None);
  let mut client = create_client(
    host.clone(),
    inbox_id.unwrap(),
    account_address.into(),
    path,
    None,
    None,
    Some(crate::client::DeviceSyncWorkerMode::Disabled),
    Some(LogOptions {
      structured: false,
      performance: true,
      level: Some(LogLevel::Info),
    }),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
  )
  .await
  .unwrap();
  let request = client.create_inbox_signature_request().unwrap().unwrap();
  let text = request.signature_text().await.unwrap();
  let signature = wallet.sign_message_sync(text.as_bytes()).unwrap();
  request
    .add_ecdsa_signature(Uint8Array::from(&signature.as_bytes()[..]))
    .await
    .unwrap();
  client.register_identity(request).await.unwrap();
  client
}

#[wasm_bindgen(js_name = createAuthTestClient)]
pub async fn create_auth_test_client(
  auth_callback: Option<AuthCallback>,
  auth_handle: Option<AuthHandle>,
) -> Result<Client, JsError> {
  // crate::opfs::Opfs::wipe_files().await.unwrap();
  let wallet = generate_local_wallet();
  let account_address = wallet.get_identifier().unwrap_throw();
  let host = GrpcUrls::NODE.to_string();
  let inbox_id = generate_inbox_id(account_address.clone().into(), None);
  let mut client = create_client(
    host.clone(),
    inbox_id.unwrap(),
    account_address.into(),
    None,
    None,
    None,
    Some(crate::client::DeviceSyncWorkerMode::Disabled),
    Some(LogOptions {
      structured: false,
      performance: true,
      level: Some(LogLevel::Trace),
    }),
    None,
    None,
    None,
    Some(GrpcUrls::GATEWAY.to_string()),
    None,
    auth_callback,
    auth_handle,
    None,
  )
  .await?;
  let request = client
    .create_inbox_signature_request()?
    .ok_or(JsError::new("Failed to create inbox signature request"))?;
  let text = request.signature_text().await?;
  let signature = wallet.sign_message_sync(text.as_bytes())?;
  request
    .add_ecdsa_signature(Uint8Array::from(signature.as_bytes().as_slice()))
    .await?;
  client.register_identity(request).await?;
  Ok(client)
}
