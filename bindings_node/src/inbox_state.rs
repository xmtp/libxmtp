use crate::{client::Client, identity::Identifier, ErrorWrapper};
use napi::bindgen_prelude::{BigInt, Result, Uint8Array};
use napi_derive::napi;
use xmtp_id::associations::{ident, AssociationState, MemberIdentifier};

#[napi(object)]
pub struct Installation {
  pub bytes: Uint8Array,
  pub client_timestamp_ns: Option<BigInt>,
  pub id: String,
}

#[napi(object)]
pub struct InboxState {
  pub inbox_id: String,
  pub recovery_identifier: Identifier,
  pub installations: Vec<Installation>,
  pub identifiers: Vec<Identifier>,
}

impl From<AssociationState> for InboxState {
  fn from(state: AssociationState) -> Self {
    let ident: Identifier = state.recovery_identifier().clone().into();
    Self {
      inbox_id: state.inbox_id().to_string(),
      recovery_identifier: ident,
      installations: state
        .members()
        .into_iter()
        .filter_map(|m| match m.identifier {
          MemberIdentifier::Ethereum(_) => None,
          MemberIdentifier::Passkey(_) => None,
          MemberIdentifier::Installation(ident::Installation(key)) => Some(Installation {
            bytes: Uint8Array::from(key.as_slice()),
            client_timestamp_ns: m.client_timestamp_ns.map(BigInt::from),
            id: hex::encode(key),
          }),
        })
        .collect(),
      identifiers: state.identifiers().into_iter().map(Into::into).collect(),
    }
  }
}

#[napi]
impl Client {
  /**
   * Get the client's inbox state.
   *
   * If `refresh_from_network` is true, the client will go to the network first to refresh the state.
   * Otherwise, the state will be read from the local database.
   */
  #[napi]
  pub async fn inbox_state(&self, refresh_from_network: bool) -> Result<InboxState> {
    let state = self
      .inner_client()
      .inbox_state(refresh_from_network)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(state.into())
  }

  #[napi]
  pub async fn get_latest_inbox_state(&self, inbox_id: String) -> Result<InboxState> {
    let conn = self
      .inner_client()
      .store()
      .conn()
      .map_err(ErrorWrapper::from)?;
    let state = self
      .inner_client()
      .get_latest_association_state(&conn, &inbox_id)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(state.into())
  }
}
