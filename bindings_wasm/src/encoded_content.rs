use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tsify::Tsify;
use xmtp_proto::xmtp::mls::message_contents::{
  ContentTypeId as XmtpContentTypeId, EncodedContent as XmtpEncodedContent,
};

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ContentTypeId {
  pub authority_id: String,
  pub type_id: String,
  pub version_major: u32,
  pub version_minor: u32,
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

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, hashmap_as_object)]
#[serde(rename_all = "camelCase")]
pub struct EncodedContent {
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub r#type: Option<ContentTypeId>,
  #[tsify(optional, type = "Record<string, string>")]
  pub parameters: HashMap<String, String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub fallback: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub compression: Option<i32>,
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub content: Vec<u8>,
}

impl From<XmtpEncodedContent> for EncodedContent {
  fn from(content: XmtpEncodedContent) -> EncodedContent {
    EncodedContent {
      r#type: content.r#type.map(Into::into),
      parameters: content.parameters,
      fallback: content.fallback,
      compression: content.compression,
      content: content.content,
    }
  }
}

impl From<EncodedContent> for XmtpEncodedContent {
  fn from(content: EncodedContent) -> Self {
    XmtpEncodedContent {
      r#type: content.r#type.map(Into::into),
      parameters: content.parameters,
      fallback: content.fallback,
      compression: content.compression,
      content: content.content,
    }
  }
}
