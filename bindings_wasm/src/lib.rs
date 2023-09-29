use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use xmtp_api_grpc_gateway::XmtpGrpcGatewayClient;

#[wasm_bindgen]
pub struct WasmXmtpClient {
    api: XmtpGrpcGatewayClient,
    // inbox_owner: WasmInboxOwner,
}

impl WasmXmtpClient {
    pub fn api(&self) -> &XmtpGrpcGatewayClient {
        &self.api
    }
}

#[wasm_bindgen]
impl WasmXmtpClient {
    #[wasm_bindgen(constructor)]
    pub fn new(url: String) -> Result<WasmXmtpClient, JsError> {
        // TODO
        Ok(WasmXmtpClient {
            api: XmtpGrpcGatewayClient::new(url),
        })
    }
}
