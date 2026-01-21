use napi::bindgen_prelude::{BigInt, Uint8Array};
use napi_derive::napi;
use xmtp_db::user_preferences::HmacKey as XmtpHmacKey;

#[napi(object)]
pub struct HmacKey {
  pub key: Uint8Array,
  pub epoch: BigInt,
}

impl From<XmtpHmacKey> for HmacKey {
  fn from(value: XmtpHmacKey) -> Self {
    Self {
      epoch: BigInt::from(value.epoch),
      key: Uint8Array::from(value.key),
    }
  }
}
