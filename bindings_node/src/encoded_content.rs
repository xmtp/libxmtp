use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use std::{collections::HashMap, ops::Deref};
use xmtp_proto::xmtp::mls::message_contents::{
  ContentTypeId as XmtpContentTypeId, EncodedContent as XmtpEncodedContent,
};

#[derive(Clone)]
#[napi(object)]
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

#[derive(Clone)]
#[napi(object)]
pub struct EncodedContent {
  pub r#type: Option<ContentTypeId>,
  pub parameters: HashMap<String, String>,
  pub fallback: Option<String>,
  pub compression: Option<i32>,
  pub content: Uint8Array,
}

#[napi]
#[allow(dead_code)]
pub fn deserialize_encoded_content(bytes: Uint8Array) -> Result<EncodedContent> {
  let encoded = XmtpEncodedContent::decode(bytes.as_ref())
    .map_err(|e| napi::Error::from_reason(format!("Failed to decode EncodedContent: {}", e)))?;
  Ok(encoded.into())
}

#[napi]
#[allow(dead_code)]
pub fn serialize_encoded_content(content: EncodedContent) -> Result<Uint8Array> {
  let encoded = XmtpEncodedContent {
    r#type: content.r#type.map(|v| v.into()),
    parameters: content.parameters,
    fallback: content.fallback,
    compression: content.compression,
    content: content.content.deref().to_vec(),
  };
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| napi::Error::from_reason(format!("Failed to serialize EncodedContent: {}", e)))?;

  Ok(buf.into())
}

impl From<XmtpEncodedContent> for EncodedContent {
  fn from(content: XmtpEncodedContent) -> EncodedContent {
    let r#type = content.r#type.map(|v| v.into());

    EncodedContent {
      r#type,
      parameters: content.parameters,
      fallback: content.fallback,
      compression: content.compression,
      content: content.content.into(),
    }
  }
}

impl From<EncodedContent> for XmtpEncodedContent {
  fn from(content: EncodedContent) -> Self {
    let r#type = content.r#type.map(|v| v.into());
    let content_bytes: Vec<u8> = content.content.deref().to_vec();

    XmtpEncodedContent {
      r#type,
      parameters: content.parameters,
      fallback: content.fallback,
      compression: content.compression,
      content: content_bytes,
    }
  }
}
