use crate::{ErrorWrapper, conversation::Conversation};
use napi::bindgen_prelude::{BigInt, Error, Result, Uint8Array};
use napi_derive::napi;
use xmtp_mls::groups::external_invite::{
  CreateExternalInviteOpts as XmtpCreateExternalInviteOpts,
  CreateExternalInviteOutput as XmtpCreateExternalInviteOutput,
};

/// Options for [`Conversation::createExternalInvite`]. Mirrors
/// [`xmtp_mls::groups::external_invite::CreateExternalInviteOpts`].
#[napi(object)]
#[derive(Default)]
pub struct CreateExternalInviteOpts {
  /// Opaque application bytes identifying the service location for the
  /// encrypted GroupInfo blob (URL, service ID, deep link, ...). libxmtp
  /// does not interpret this; the receiving application does.
  pub service_pointer: Option<Uint8Array>,
  /// Advisory expiry as nanoseconds since UNIX epoch. `None` means no
  /// expiry; the storage service is the hard enforcement point.
  pub expires_at_ns: Option<BigInt>,
}

/// Output of [`Conversation::createExternalInvite`]. Both fields are raw
/// protobuf-serialized bytes; the application picks its own encoding for
/// transport.
#[napi(object)]
pub struct CreateExternalInviteOutput {
  /// Serialized `ExternalInvitePayload` proto. The application encodes
  /// this however it wants (hex, base64, raw QR, NFC, deep link, ...)
  /// and embeds it in the shareable invite.
  pub invite_payload: Uint8Array,
  /// Serialized `EncryptedGroupInfoBlob` proto. The application uploads
  /// this to its service indexed by the payload's `group_id_hash`.
  pub encrypted_group_info: Uint8Array,
}

impl From<XmtpCreateExternalInviteOutput> for CreateExternalInviteOutput {
  fn from(value: XmtpCreateExternalInviteOutput) -> Self {
    Self {
      invite_payload: Uint8Array::from(value.invite_payload),
      encrypted_group_info: Uint8Array::from(value.encrypted_group_info),
    }
  }
}

#[napi]
impl Conversation {
  /// Produce a QR-invite payload + encrypted GroupInfo blob for the
  /// current epoch of this group. Pair with an external service that
  /// stores the encrypted blob keyed by the payload's `group_id_hash`.
  #[napi]
  pub async fn create_external_invite(
    &self,
    opts: Option<CreateExternalInviteOpts>,
  ) -> Result<CreateExternalInviteOutput> {
    let opts = opts.unwrap_or_default();

    let service_pointer = opts.service_pointer.map(|b| b.to_vec()).unwrap_or_default();

    let expires_at_ns = match opts.expires_at_ns {
      Some(big) => {
        let (signed, value, lossless) = big.get_u64();
        if signed {
          return Err(Error::from_reason("`expiresAtNs` must be non-negative"));
        }
        if !lossless {
          return Err(Error::from_reason("`expiresAtNs` is too large for u64"));
        }
        Some(value)
      }
      None => None,
    };

    let group = self.create_mls_group();
    let output = group
      .create_external_invite(XmtpCreateExternalInviteOpts {
        service_pointer,
        blob_expires_at_ns: expires_at_ns,
      })
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(output.into())
  }
}
