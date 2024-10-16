use js_sys::Uint8Array;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct WasmContentTypeId {
  pub authority_id: String,
  pub type_id: String,
  pub version_major: u32,
  pub version_minor: u32,
}

impl From<ContentTypeId> for WasmContentTypeId {
  fn from(content_type_id: ContentTypeId) -> WasmContentTypeId {
    WasmContentTypeId {
      authority_id: content_type_id.authority_id,
      type_id: content_type_id.type_id,
      version_major: content_type_id.version_major,
      version_minor: content_type_id.version_minor,
    }
  }
}

impl From<WasmContentTypeId> for ContentTypeId {
  fn from(content_type_id: WasmContentTypeId) -> Self {
    ContentTypeId {
      authority_id: content_type_id.authority_id,
      type_id: content_type_id.type_id,
      version_major: content_type_id.version_major,
      version_minor: content_type_id.version_minor,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct WasmEncodedContent {
  pub r#type: Option<WasmContentTypeId>,
  pub parameters: JsValue,
  pub fallback: Option<String>,
  pub compression: Option<i32>,
  pub content: Uint8Array,
}

impl From<EncodedContent> for WasmEncodedContent {
  fn from(content: EncodedContent) -> WasmEncodedContent {
    let r#type = content.r#type.map(|v| v.into());

    WasmEncodedContent {
      r#type,
      parameters: serde_wasm_bindgen::to_value(&content.parameters).unwrap(),
      fallback: content.fallback,
      compression: content.compression,
      content: content.content.as_slice().into(),
    }
  }
}

impl From<WasmEncodedContent> for EncodedContent {
  fn from(content: WasmEncodedContent) -> Self {
    let r#type = content.r#type.map(|v| v.into());

    EncodedContent {
      r#type,
      parameters: serde_wasm_bindgen::from_value(content.parameters).unwrap(),
      fallback: content.fallback,
      compression: content.compression,
      content: content.content.to_vec(),
    }
  }
}
