use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
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
