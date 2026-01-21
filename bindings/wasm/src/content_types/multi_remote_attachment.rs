use super::remote_attachment::RemoteAttachment;
use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec;
use xmtp_proto::xmtp::mls::message_contents::content_types::MultiRemoteAttachment as XmtpMultiRemoteAttachment;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MultiRemoteAttachment {
  pub attachments: Vec<RemoteAttachment>,
}

impl From<MultiRemoteAttachment> for XmtpMultiRemoteAttachment {
  fn from(multi: MultiRemoteAttachment) -> Self {
    XmtpMultiRemoteAttachment {
      attachments: multi.attachments.into_iter().map(Into::into).collect(),
    }
  }
}

impl From<XmtpMultiRemoteAttachment> for MultiRemoteAttachment {
  fn from(multi: XmtpMultiRemoteAttachment) -> Self {
    MultiRemoteAttachment {
      attachments: multi.attachments.into_iter().map(Into::into).collect(),
    }
  }
}

#[wasm_bindgen(js_name = "contentTypeMultiRemoteAttachment")]
pub fn content_type_multi_remote_attachment() -> ContentTypeId {
  MultiRemoteAttachmentCodec::content_type().into()
}

#[wasm_bindgen(js_name = "encodeMultiRemoteAttachment")]
pub fn encode_multi_remote_attachment(
  multi_remote_attachment: MultiRemoteAttachment,
) -> Result<EncodedContent, JsError> {
  Ok(
    MultiRemoteAttachmentCodec::encode(multi_remote_attachment.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?
      .into(),
  )
}
