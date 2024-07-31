use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use xmtp_api_mls_gateway::XmtpApiMlsGateway;

#[wasm_bindgen]
pub struct WasmXmtpClient {
  inner: XmtpApiMlsGateway,
}

impl WasmXmtpClient {
  pub fn api_client(&self) -> &XmtpApiMlsGateway {
    &self.inner
  }
}

#[wasm_bindgen]
impl WasmXmtpClient {
  #[wasm_bindgen(constructor)]
  pub fn create_client(host_url: String) -> Result<WasmXmtpClient, JsError> {
    Ok(WasmXmtpClient {
      inner: XmtpApiMlsGateway::new(host_url),
    })
  }
}
