use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::text::TextCodec as XmtpTextCodec;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TextContent {
  pub content: String,
}

impl From<xmtp_mls::messages::decoded_message::Text> for TextContent {
  fn from(text: xmtp_mls::messages::decoded_message::Text) -> Self {
    Self {
      content: text.content,
    }
  }
}

#[wasm_bindgen]
pub struct TextCodec;

#[wasm_bindgen]
impl TextCodec {
  #[wasm_bindgen(js_name = "contentType")]
  pub fn content_type() -> ContentTypeId {
    XmtpTextCodec::content_type().into()
  }

  #[wasm_bindgen]
  pub fn encode(text: String) -> Result<EncodedContent, JsError> {
    let encoded_content =
      XmtpTextCodec::encode(text).map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(encoded_content.into())
  }

  #[wasm_bindgen]
  pub fn decode(encoded_content: EncodedContent) -> Result<String, JsError> {
    XmtpTextCodec::decode(encoded_content.into()).map_err(|e| JsError::new(&format!("{}", e)))
  }

  #[wasm_bindgen(js_name = "shouldPush")]
  pub fn should_push() -> bool {
    XmtpTextCodec::should_push()
  }
}
