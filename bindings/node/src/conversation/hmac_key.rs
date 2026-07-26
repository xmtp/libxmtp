use crate::{ErrorWrapper, conversation::Conversation, hmac_key::HmacKey};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use std::collections::HashMap;

#[napi]
impl Conversation {
  #[napi]
  #[xmtp_common::err_span]
  pub async fn hmac_keys(&self) -> Result<HashMap<String, Vec<HmacKey>>> {
    let group = self.create_mls_group();

    let dms = group
      .find_duplicate_dms()
      .await
      .map_err(ErrorWrapper::from)?;

    let mut hmac_map = HashMap::new();
    for conversation in dms {
      let id = hex::encode(conversation.group_id);
      let keys = conversation
        .hmac_keys(-1..=1)
        .await
        .map_err(ErrorWrapper::from)?
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();
      hmac_map.insert(id, keys);
    }

    let keys = group
      .hmac_keys(-1..=1)
      .await
      .map_err(ErrorWrapper::from)?
      .into_iter()
      .map(Into::into)
      .collect::<Vec<_>>();

    hmac_map.insert(self.id(), keys);

    Ok(hmac_map)
  }
}
