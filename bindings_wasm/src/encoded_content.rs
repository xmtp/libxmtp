use js_sys::Uint8Array;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;
use xmtp_proto::xmtp::mls::message_contents::{
  ContentTypeId as XmtpContentTypeId, EncodedContent as XmtpEncodedContent,
};

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct ContentTypeId {
  #[wasm_bindgen(js_name = authorityId)]
  pub authority_id: String,
  #[wasm_bindgen(js_name = typeId)]
  pub type_id: String,
  #[wasm_bindgen(js_name = versionMajor)]
  pub version_major: u32,
  #[wasm_bindgen(js_name = versionMinor)]
  pub version_minor: u32,
}

#[wasm_bindgen]
impl ContentTypeId {
  #[wasm_bindgen(constructor)]
  pub fn new(
    authority_id: String,
    type_id: String,
    version_major: u32,
    version_minor: u32,
  ) -> Self {
    Self {
      authority_id,
      type_id,
      version_major,
      version_minor,
    }
  }
}

impl From<XmtpContentTypeId> for ContentTypeId {
  fn from(content_type_id: XmtpContentTypeId) -> ContentTypeId {
    ContentTypeId {
      authority_id: content_type_id.authority_id,
      type_id: content_type_id.type_id,
      version_major: content_type_id.version_major,
      version_minor: content_type_id.version_minor,
    }
  }
}

impl From<ContentTypeId> for XmtpContentTypeId {
  fn from(content_type_id: ContentTypeId) -> Self {
    XmtpContentTypeId {
      authority_id: content_type_id.authority_id,
      type_id: content_type_id.type_id,
      version_major: content_type_id.version_major,
      version_minor: content_type_id.version_minor,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct EncodedContent {
  pub r#type: Option<ContentTypeId>,
  pub parameters: JsValue,
  pub fallback: Option<String>,
  pub compression: Option<i32>,
  pub content: Uint8Array,
}

#[wasm_bindgen]
impl EncodedContent {
  #[wasm_bindgen(constructor)]
  pub fn new(
    r#type: Option<ContentTypeId>,
    parameters: JsValue,
    fallback: Option<String>,
    compression: Option<i32>,
    content: Uint8Array,
  ) -> EncodedContent {
    EncodedContent {
      r#type,
      parameters,
      fallback,
      compression,
      content,
    }
  }
}

impl From<XmtpEncodedContent> for EncodedContent {
  fn from(content: XmtpEncodedContent) -> EncodedContent {
    let r#type = content.r#type.map(|v| v.into());

    EncodedContent {
      r#type,
      parameters: serde_wasm_bindgen::to_value(&content.parameters).unwrap(),
      fallback: content.fallback,
      compression: content.compression,
      content: content.content.as_slice().into(),
    }
  }
}

impl From<EncodedContent> for XmtpEncodedContent {
  fn from(content: EncodedContent) -> Self {
    let r#type = content.r#type.map(|v| v.into());

    XmtpEncodedContent {
      r#type,
      parameters: serde_wasm_bindgen::from_value(content.parameters).unwrap(),
      fallback: content.fallback,
      compression: content.compression,
      content: content.content.to_vec(),
    }
  }
}
