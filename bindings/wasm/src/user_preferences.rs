use crate::consent_state::Consent;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use xmtp_mls::groups::device_sync::preference_sync::PreferenceUpdate;

#[derive(Tsify, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type")]
pub enum UserPreferenceUpdate {
  ConsentUpdate {
    consent: Consent,
  },
  HmacKeyUpdate {
    // serde bytes converts to Uint8Array
    #[serde(with = "serde_bytes")]
    key: Vec<u8>,
  },
}

impl From<PreferenceUpdate> for UserPreferenceUpdate {
  fn from(v: PreferenceUpdate) -> UserPreferenceUpdate {
    match v {
      PreferenceUpdate::Consent(c) => UserPreferenceUpdate::ConsentUpdate {
        consent: Consent::from(c),
      },
      PreferenceUpdate::Hmac { key, .. } => UserPreferenceUpdate::HmacKeyUpdate { key },
    }
  }
}
