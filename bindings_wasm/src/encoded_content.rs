use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tsify_next::Tsify;
use xmtp_proto::xmtp::mls::message_contents::{
  ContentTypeId as XmtpContentTypeId, EncodedContent as XmtpEncodedContent,
};

#[derive(Tsify, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ContentTypeId {
  #[serde(rename = "authorityId")]
  pub authority_id: String,
  #[serde(rename = "typeId")]
  pub type_id: String,
  #[serde(rename = "versionMajor")]
  pub version_major: u32,
  #[serde(rename = "versionMinor")]
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

#[derive(Tsify, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct EncodedContent {
  pub r#type: Option<ContentTypeId>,
  pub parameters: HashMap<String, String>,
  pub fallback: Option<String>,
  pub compression: Option<i32>,
  #[serde(with = "serde_bytes")]
  pub content: Vec<u8>,
}

impl From<XmtpEncodedContent> for EncodedContent {
  fn from(content: XmtpEncodedContent) -> EncodedContent {
    let r#type = content.r#type.map(|v| v.into());

    EncodedContent {
      r#type,
      parameters: content.parameters,
      fallback: content.fallback,
      compression: content.compression,
      content: content.content,
    }
  }
}

impl From<EncodedContent> for XmtpEncodedContent {
  fn from(content: EncodedContent) -> Self {
    let r#type = content.r#type.map(|v| v.into());

    XmtpEncodedContent {
      r#type,
      parameters: content.parameters,
      fallback: content.fallback,
      compression: content.compression,
      content: content.content.to_vec(),
    }
  }
}
