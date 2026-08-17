use crate::{ErrorWrapper, consent_state::ConsentState, conversation::Conversation};
use napi::bindgen_prelude::Result;
use napi_derive::napi;

#[napi]
impl Conversation {
  #[napi]
  #[xmtp_common::err_span]
  pub async fn consent_state(&self) -> Result<ConsentState> {
    let group = self.create_mls_group();

    let state = group.consent_state().await.map_err(ErrorWrapper::from)?;

    Ok(state.into())
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn update_consent_state(&self, state: ConsentState) -> Result<()> {
    let group = self.create_mls_group();

    group
      .update_consent_state(state.into())
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }
}
