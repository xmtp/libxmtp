use napi::bindgen_prelude::Uint8Array;
use napi_derive::napi;
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
