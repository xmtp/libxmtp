use crate::ErrorWrapper;
use crate::conversations::Conversations;
use crate::hmac_key::HmacKey;
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use std::collections::HashMap;
use xmtp_db::group::GroupQueryArgs;

#[napi]
impl Conversations {
  #[napi]
  pub fn get_hmac_keys(&self) -> Result<HashMap<String, Vec<HmacKey>>> {
    let inner = self.inner_client.as_ref();
    let conversations = inner
      .find_groups(GroupQueryArgs {
        include_duplicate_dms: true,
        ..Default::default()
      })
      .map_err(ErrorWrapper::from)?;

    let mut hmac_map = HashMap::new();
    for conversation in conversations {
      let id = hex::encode(&conversation.group_id);
      let keys = conversation
        .hmac_keys(-1..=1)
        .map_err(ErrorWrapper::from)?
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();
      hmac_map.insert(id, keys);
    }

    Ok(hmac_map)
  }
}
