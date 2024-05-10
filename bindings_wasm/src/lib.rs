// use std::collectioashMap;

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
    pub fn create_client(host_url: String) -> Result<WasmXmtpClient, JsError> {
        Ok(WasmXmtpClient {
            api: XmtpGrpcGatewayClient::new(host_url),
        })
    }

    // #[wasm_bindgen]
    // pub async fn can_message(
    //     &self,
    //     _account_addresses: Vec<String>,
    // ) -> Result<HashMap<String, bool>, JsError> {
    //     let allowed = HashMap::new();
    //     Ok(allowed)
    // }
}
