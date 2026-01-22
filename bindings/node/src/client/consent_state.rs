use crate::consent_state::Consent;
use crate::consent_state::ConsentEntityType;
use crate::consent_state::ConsentState;
use crate::{ErrorWrapper, client::Client};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_db::consent_record::StoredConsentRecord;

#[napi]
impl Client {
  #[napi]
  pub async fn set_consent_states(&self, records: Vec<Consent>) -> Result<()> {
    let stored_records: Vec<StoredConsentRecord> =
      records.into_iter().map(StoredConsentRecord::from).collect();

    self
      .inner_client()
      .set_consent_states(&stored_records)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn get_consent_state(
    &self,
    entity_type: ConsentEntityType,
    entity: String,
  ) -> Result<ConsentState> {
    let result = self
      .inner_client()
      .get_consent_state(entity_type.into(), entity)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(result.into())
  }
}
