use crate::ErrorWrapper;
use crate::conversation::Conversation;
use crate::conversation::disappearing_messages::MessageDisappearingSettings;
use crate::conversations::Conversations;
use crate::identity::Identifier;
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_mls::mls_common::group::DMMetadataOptions;

#[napi(object)]
#[derive(Clone, Default)]
pub struct CreateDMOptions {
  pub message_disappearing_settings: Option<MessageDisappearingSettings>,
}

impl CreateDMOptions {
  pub fn into_dm_metadata_options(self) -> DMMetadataOptions {
    DMMetadataOptions {
      message_disappearing_settings: self
        .message_disappearing_settings
        .map(|settings| settings.into()),
    }
  }
}

#[napi]
impl Conversations {
  #[napi(js_name = "createDmByIdentity")]
  pub async fn find_or_create_dm_by_identity(
    &self,
    account_identity: Identifier,
    options: Option<CreateDMOptions>,
  ) -> Result<Conversation> {
    let convo = self
      .inner_client
      .find_or_create_dm(
        account_identity.try_into()?,
        options.map(|opt| opt.into_dm_metadata_options()),
      )
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(convo.into())
  }

  #[napi(js_name = "createDm")]
  pub async fn find_or_create_dm(
    &self,
    inbox_id: String,
    options: Option<CreateDMOptions>,
  ) -> Result<Conversation> {
    let convo = self
      .inner_client
      .find_or_create_dm_by_inbox_id(inbox_id, options.map(|opt| opt.into_dm_metadata_options()))
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(convo.into())
  }

  #[napi]
  pub fn find_dm_by_target_inbox_id(&self, target_inbox_id: String) -> Result<Conversation> {
    let convo = self
      .inner_client
      .dm_group_from_target_inbox(target_inbox_id)
      .map_err(ErrorWrapper::from)?;

    Ok(convo.into())
  }
}
