use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;

use super::Client;
use crate::ErrorWrapper;
use crate::conversation::Conversation;

/// Output of [`Client::join_group_by_external_invite`].
///
/// Exposes the newly-joined group (via the `conversation` getter) and the
/// post-commit re-encrypted GroupInfo blob (`refreshedEncryptedGroupInfo`).
/// The application should upload `refreshedEncryptedGroupInfo` back to its
/// invite service under the same `groupIdHash` key the original blob used
/// (overwrite semantics) so the next joiner reads a GroupInfo at the new
/// epoch — otherwise their external commit would race a stale ratchet tree.
#[napi]
pub struct JoinGroupByExternalInviteOutput {
  conversation: Conversation,
  refreshed_encrypted_group_info: Vec<u8>,
}

#[napi]
impl JoinGroupByExternalInviteOutput {
  #[napi(getter)]
  pub fn conversation(&self) -> Conversation {
    self.conversation.clone()
  }

  #[napi(getter)]
  pub fn refreshed_encrypted_group_info(&self) -> Uint8Array {
    self.refreshed_encrypted_group_info.as_slice().into()
  }
}

#[napi]
impl Client {
  /// Join a group via an external invite (atomic external commit).
  ///
  /// `invitePayload` is the serialized `ExternalInvitePayload` proto carried
  /// by the QR-code or link. `encryptedGroupInfo` is the encrypted blob the
  /// application fetched from its invite service using the payload's
  /// `groupIdHash` as the lookup key.
  ///
  /// On success returns the joined [`Conversation`] together with a
  /// post-commit `refreshedEncryptedGroupInfo` blob that the caller should
  /// ship back to the invite service so the next joiner reads a GroupInfo
  /// at the new epoch.
  #[napi]
  pub async fn join_group_by_external_invite(
    &self,
    invite_payload: Uint8Array,
    encrypted_group_info: Uint8Array,
  ) -> Result<JoinGroupByExternalInviteOutput> {
    let output = self
      .inner_client
      .join_group_by_external_invite(invite_payload.as_ref(), encrypted_group_info.as_ref())
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(JoinGroupByExternalInviteOutput {
      conversation: output.group.into(),
      refreshed_encrypted_group_info: output.refreshed_encrypted_group_info,
    })
  }
}
