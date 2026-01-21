use crate::{ErrorWrapper, conversation::Conversation};
use napi::bindgen_prelude::{BigInt, Result};
use napi_derive::napi;
use xmtp_mls::groups::ConversationDebugInfo as XmtpConversationDebugInfo;
use xmtp_proto::types::Cursor as XmtpCursor;

#[napi(object)]
pub struct Cursor {
  pub originator_id: u32,
  // napi doesn't support u64
  pub sequence_id: i64,
}

impl From<XmtpCursor> for Cursor {
  fn from(value: XmtpCursor) -> Self {
    Self {
      originator_id: value.originator_id,
      sequence_id: value.sequence_id as i64,
    }
  }
}

#[napi(object)]
pub struct ConversationDebugInfo {
  pub epoch: BigInt,
  pub maybe_forked: bool,
  pub fork_details: String,
  pub is_commit_log_forked: Option<bool>,
  pub local_commit_log: String,
  pub remote_commit_log: String,
  pub cursor: Vec<Cursor>,
}

impl From<XmtpConversationDebugInfo> for ConversationDebugInfo {
  fn from(value: XmtpConversationDebugInfo) -> Self {
    Self {
      epoch: BigInt::from(value.epoch),
      maybe_forked: value.maybe_forked,
      fork_details: value.fork_details,
      is_commit_log_forked: value.is_commit_log_forked,
      local_commit_log: value.local_commit_log,
      remote_commit_log: value.remote_commit_log,
      cursor: value.cursor.into_iter().map(Into::into).collect(),
    }
  }
}

#[napi]
impl Conversation {
  #[napi]
  pub async fn debug_info(&self) -> Result<ConversationDebugInfo> {
    let group = self.create_mls_group();

    Ok(
      group
        .debug_info()
        .await
        .map(Into::into)
        .map_err(ErrorWrapper::from)?,
    )
  }
}
