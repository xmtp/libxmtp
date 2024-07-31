use bindings_wasm::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
use xmtp_api_mls_gateway::constants::ApiUrls;
use xmtp_proto::{
  api_client::XmtpMlsClient,
  xmtp::mls::api::v1::{KeyPackageUpload, RegisterInstallationRequest},
};

// Only run these tests in a browser.
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen]
extern "C" {
  #[wasm_bindgen(js_namespace = console)]
  fn log(s: &str);
}

#[wasm_bindgen_test]
pub async fn test_client_raw_requests() {
  let client =
    WasmXmtpClient::create_client(ApiUrls::LOCAL_ADDRESS.to_string()).unwrap_or_else(|error| {
      let error_str = format!("{:?}", JsValue::from(error));
      log(&error_str);
      panic!("client should be constructed");
    });

  let api_client = client.api_client();
  let res = api_client
    .register_installation(RegisterInstallationRequest {
      is_inbox_id_credential: false,
      key_package: Some(KeyPackageUpload {
        key_package_tls_serialized: vec![1, 2, 3],
      }),
    })
    .await;

  assert!(res.is_err());
  let error_string = format!("{:?}", JsValue::from(res.err().unwrap().to_string()));
  log(&error_string);
  //   assert!(error_string.contains("invalid identity"));
}
