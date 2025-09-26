use crate::ConversionError;
use chrono::Local;
use derive_builder::Builder;

use crate::types::{Cursor, InstallationId};

/// Welcome Message from the network
#[derive(Clone, Builder, Debug)]
#[builder(setter(into), build_fn(error = "ConversionError"))]
pub struct WelcomeMessage {
    /// cursor of this message
    pub cursor: Cursor,
    /// server timestamp indicating when this message was created
    pub created_ns: chrono::DateTime<Local>,
    /// Installation key of user sending the welcome
    pub installation_key: InstallationId,
    /// welcome message payload
    pub data: Vec<u8>,
    /// HPKE Public Key
    pub hpke_public_key: Vec<u8>,
    /// Welcome Wrapper Algorithm
    pub wrapper_algorithm: i32,
    /// Extra metadata attached to welcome
    pub welcome_metadata: Vec<u8>,
}

impl WelcomeMessage {
    pub fn builder() -> WelcomeMessageBuilder {
        WelcomeMessageBuilder::default()
    }
}

impl WelcomeMessage {
    pub fn sequence_id(&self) -> u64 {
        self.cursor.sequence_id
    }

    pub fn originator_id(&self) -> u32 {
        self.cursor.originator_id
    }

    pub fn timestamp(&self) -> i64 {
        self.created_ns
            .timestamp_nanos_opt()
            .expect("timestamp out of range for i64, are we in 2262 A.D?")
    }
}

#[cfg(any(test, feature = "test-utils"))]
mod test {
    use super::*;
    use crate::xmtp::mls::message_contents::WelcomeWrapperAlgorithm;
    use xmtp_common::{rand_array, rand_i64, rand_vec, Generate};

    impl Generate for WelcomeMessage {
        fn generate() -> Self {
            Self {
                cursor: Cursor::generate(),
                created_ns: chrono::DateTime::from_timestamp_nanos(rand_i64()).into(),
                installation_key: rand_array::<32>().into(),
                data: rand_vec::<16>(),
                hpke_public_key: rand_vec::<16>(),
                wrapper_algorithm: WelcomeWrapperAlgorithm::Curve25519.into(),
                welcome_metadata: rand_vec::<16>(),
            }
        }
    }
}
