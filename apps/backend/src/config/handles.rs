//! Handle settings. The backend applies them only when it admits a claim.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use xmtp_configuration::BACKEND_DEFAULT_HANDLE_MIN_NAME_LENGTH;
use xmtp_id::associations::MAX_HANDLE_NAME_LEN;

use super::{ConfigError, invalid};

/// The longest name part, as the type of the configured minimum.
const MAX_MIN_NAME_LENGTH: u32 = MAX_HANDLE_NAME_LEN as u32;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct HandlesConfig {
    /// The shortest name part, the text before `#`, admitted in a new claim.
    /// A change applies only to later claims; handles claimed before it stay
    /// valid.
    #[schemars(range(min = 1, max = MAX_MIN_NAME_LENGTH))]
    pub min_name_length: u32,
}

impl Default for HandlesConfig {
    fn default() -> Self {
        Self {
            min_name_length: BACKEND_DEFAULT_HANDLE_MIN_NAME_LENGTH,
        }
    }
}

impl HandlesConfig {
    /// Refuse a minimum that would publish as "not provided" (0) or that no
    /// name part can meet (above the longest name part).
    // implements: CONF-080
    pub(super) fn validate(&self) -> Result<(), ConfigError> {
        if !(1..=MAX_MIN_NAME_LENGTH).contains(&self.min_name_length) {
            return Err(invalid("handles.min_name_length", "must be 1 to 27"));
        }
        Ok(())
    }
}
