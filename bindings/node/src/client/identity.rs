use crate::ErrorWrapper;
use crate::client::Client;
use crate::identity::Identifier;
use crate::signatures::SignatureRequestHandle;
use napi::bindgen_prelude::{Error, Result, Uint8Array};
use napi_derive::napi;

#[napi]
impl Client {
  #[napi(getter)]
  pub fn account_identifier(&self) -> Identifier {
    self.account_identifier.clone()
  }

  #[napi]
  pub fn inbox_id(&self) -> String {
    self.inner_client.inbox_id().to_string()
  }

  #[napi]
  pub fn is_registered(&self) -> bool {
    self.inner_client.identity().is_ready()
  }

  #[napi]
  pub fn installation_id(&self) -> String {
    hex::encode(self.inner_client.installation_public_key())
  }

  #[napi]
  pub fn installation_id_bytes(&self) -> Uint8Array {
    self.inner_client.installation_public_key().into()
  }

  #[napi]
  pub async fn register_identity(&self, signature_request: &SignatureRequestHandle) -> Result<()> {
    if self.is_registered() {
      return Err(Error::from_reason(
        "An identity is already registered with this client",
      ));
    }

    let inner = signature_request.inner().lock().await;

    self
      .inner_client
      .register_identity(inner.clone())
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn get_inbox_id_by_identity(&self, identifier: Identifier) -> Result<Option<String>> {
    let conn = self.inner_client().context.store().db();

    let inbox_id = self
      .inner_client
      .find_inbox_id_from_identifier(&conn, identifier.try_into()?)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(inbox_id)
  }
}
