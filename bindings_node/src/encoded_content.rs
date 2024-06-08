use napi::bindgen_prelude::Uint8Array;
use napi_derive::napi;
use std::{collections::HashMap, ops::Deref};
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

#[derive(Clone)]
#[napi(object)]
pub struct NapiContentTypeId {
  pub authority_id: String,
  pub type_id: String,
  pub version_major: u32,
  pub version_minor: u32,
}

impl From<ContentTypeId> for NapiContentTypeId {
  fn from(content_type_id: ContentTypeId) -> NapiContentTypeId {
    NapiContentTypeId {
      authority_id: content_type_id.authority_id,
      type_id: content_type_id.type_id,
      version_major: content_type_id.version_major,
      version_minor: content_type_id.version_minor,
    }
  }
}

impl From<NapiContentTypeId> for ContentTypeId {
  fn from(content_type_id: NapiContentTypeId) -> Self {
    ContentTypeId {
      authority_id: content_type_id.authority_id,
      type_id: content_type_id.type_id,
      version_major: content_type_id.version_major,
      version_minor: content_type_id.version_minor,
    }
  }
}

#[derive(Clone)]
#[napi(object)]
pub struct NapiEncodedContent {
  pub r#type: Option<NapiContentTypeId>,
  pub parameters: HashMap<String, String>,
  pub fallback: Option<String>,
  pub compression: Option<i32>,
  pub content: Uint8Array,
}

impl From<EncodedContent> for NapiEncodedContent {
  fn from(content: EncodedContent) -> NapiEncodedContent {
    let r#type = content.r#type.map(|v| v.into());

    NapiEncodedContent {
      r#type,
      parameters: content.parameters,
      fallback: content.fallback,
      compression: content.compression,
      content: content.content.into(),
    }
  }
}

impl From<NapiEncodedContent> for EncodedContent {
  fn from(content: NapiEncodedContent) -> Self {
    let r#type = content.r#type.map(|v| v.into());
    let content_bytes: Vec<u8> = content.content.deref().to_vec();

    EncodedContent {
      r#type,
      parameters: content.parameters,
      fallback: content.fallback,
      compression: content.compression,
      content: content_bytes,
    }
  }
}
