use crate::{ErrorWrapper, conversation::Conversation};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_db::group::DmIdExt;

#[napi]
impl Conversation {
  #[napi]
  pub fn dm_peer_inbox_id(&self) -> Result<String> {
    let group = self.create_mls_group();
    let inbox_id = group.context.inbox_id();
    let dm_id = group.dm_id.as_ref().ok_or(napi::Error::from_reason(
      "Not a DM conversation or missing DM ID",
    ))?;

    Ok(dm_id.other_inbox_id(inbox_id))
  }

  #[napi]
  pub async fn duplicate_dms(&self) -> Result<Vec<Conversation>> {
    let group = self.create_mls_group();
    let dms = group.find_duplicate_dms().map_err(ErrorWrapper::from)?;
    let conversations: Vec<Conversation> = dms.into_iter().map(Into::into).collect();

    Ok(conversations)
  }
}
