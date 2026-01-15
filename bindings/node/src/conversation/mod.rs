use crate::{ErrorWrapper, client::RustMlsGroup};
use napi::bindgen_prelude::{BigInt, Result};
use napi_derive::napi;
use xmtp_mls::groups::MlsGroup;

pub mod consent_state;
pub mod content_types;
pub mod debug;
pub mod disappearing_messages;
pub mod dm;
pub mod group;
pub mod hmac_key;
pub mod messages;
pub mod permissions;
pub mod streams;

#[napi]
#[derive(Clone)]
pub struct Conversation {
  inner_group: RustMlsGroup,
  group_id: Vec<u8>,
  dm_id: Option<String>,
  created_at_ns: BigInt,
}

impl From<RustMlsGroup> for Conversation {
  fn from(mls_group: RustMlsGroup) -> Self {
    Conversation {
      group_id: mls_group.group_id.clone(),
      dm_id: mls_group.dm_id.clone(),
      created_at_ns: BigInt::from(mls_group.created_at_ns),
      inner_group: mls_group,
    }
  }
}

#[napi]
impl Conversation {
  pub fn new(
    inner_group: RustMlsGroup,
    group_id: Vec<u8>,
    dm_id: Option<String>,
    created_at_ns: BigInt,
  ) -> Self {
    Self {
      inner_group,
      group_id,
      dm_id,
      created_at_ns,
    }
  }

  // helper method to create a new MlsGroup
  pub fn create_mls_group(&self) -> RustMlsGroup {
    MlsGroup::new(
      self.inner_group.context.clone(),
      self.group_id.clone(),
      self.dm_id.clone(),
      self.inner_group.conversation_type,
      self.created_at_ns.get_i64().0,
    )
  }

  #[napi]
  pub fn id(&self) -> String {
    hex::encode(self.group_id.clone())
  }

  #[napi]
  pub fn created_at_ns(&self) -> BigInt {
    self.created_at_ns.clone()
  }

  #[napi]
  pub fn is_active(&self) -> Result<bool> {
    let group = self.create_mls_group();

    Ok(group.is_active().map_err(ErrorWrapper::from)?)
  }

  #[napi]
  pub fn paused_for_version(&self) -> napi::Result<Option<String>> {
    let group = self.create_mls_group();

    Ok(group.paused_for_version().map_err(ErrorWrapper::from)?)
  }

  #[napi]
  pub async fn sync(&self) -> Result<()> {
    let group = self.create_mls_group();
    group.sync().await.map_err(ErrorWrapper::from)?;

    Ok(())
  }
}
