use crate::consent_state::Consent;
use serde::{Deserialize, Serialize};
use tsify_next::Tsify;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_mls::groups::device_sync::preference_sync::UserPreferenceUpdate;

#[derive(Tsify, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type")]
pub enum UserPreference {
  Consent {
    consent: Consent,
  },
  HmacKeyUpdate {
    // serde bytes converts to Uint8Array
    #[serde(with = "serde_bytes")]
    key: Vec<u8>,
  },
}

impl From<UserPreferenceUpdate> for UserPreference {
  fn from(v: UserPreferenceUpdate) -> UserPreference {
    match v {
      UserPreferenceUpdate::ConsentUpdate(c) => UserPreference::Consent {
        consent: Consent::from(c),
      },
      UserPreferenceUpdate::HmacKeyUpdate { key } => UserPreference::HmacKeyUpdate { key },
    }
  }
}
