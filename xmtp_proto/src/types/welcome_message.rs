use crate::ConversionError;
use chrono::Utc;
use derive_builder::Builder;

use crate::types::{Cursor, InstallationId};

/// Welcome Message from the network
#[derive(Clone, Builder, Debug)]
#[builder(setter(into), build_fn(error = "ConversionError"))]
pub struct WelcomeMessage {
    /// cursor of this message
    pub cursor: Cursor,
    /// server timestamp indicating when this message was created
    pub created_ns: chrono::DateTime<Utc>,
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
impl xmtp_common::Generate for WelcomeMessage {
    fn generate() -> Self {
        Self {
            cursor: Cursor::generate(),
            created_ns: chrono::DateTime::from_timestamp_nanos(xmtp_common::rand_i64()),
            installation_key: xmtp_common::rand_array::<32>().into(),
            data: xmtp_common::rand_vec::<16>(),
            hpke_public_key: xmtp_common::rand_vec::<16>(),
            wrapper_algorithm:
                crate::xmtp::mls::message_contents::WelcomeWrapperAlgorithm::Curve25519.into(),
            welcome_metadata: xmtp_common::rand_vec::<16>(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::xmtp::mls::message_contents::WelcomeWrapperAlgorithm;
    use rstest::rstest;
    use xmtp_common::Generate;

    #[rstest]
    #[case(Cursor::new(123, 456u32), 123, 456u32)]
    #[case(Cursor::new(0, 0u32), 0, 0u32)]
    #[case(Cursor::new(u64::MAX, u32::MAX), u64::MAX, u32::MAX)]
    #[xmtp_common::test]
    fn test_accessor_methods(
        #[case] cursor: Cursor,
        #[case] expected_seq: u64,
        #[case] expected_orig: u32,
    ) {
        use xmtp_common::Generate;

        let mut welcome_message = WelcomeMessage::generate();
        welcome_message.cursor = cursor;
        assert_eq!(welcome_message.sequence_id(), expected_seq);
        assert_eq!(welcome_message.originator_id(), expected_orig);
    }

    #[xmtp_common::test]
    fn test_timestamp() {
        let test_time = chrono::Utc::now();
        let mut welcome_message = WelcomeMessage::generate();
        welcome_message.created_ns = test_time;
        assert_eq!(
            welcome_message.timestamp(),
            test_time.timestamp_nanos_opt().unwrap()
        );
    }

    #[rstest]
    #[case(WelcomeWrapperAlgorithm::Curve25519)]
    #[case(WelcomeWrapperAlgorithm::XwingMlkem768Draft6)]
    #[case(WelcomeWrapperAlgorithm::Unspecified)]
    #[xmtp_common::test]
    fn test_wrapper_algorithms(#[case] algorithm: WelcomeWrapperAlgorithm) {
        let mut welcome_message = WelcomeMessage::generate();
        let algorithm_i32: i32 = algorithm.into();
        welcome_message.wrapper_algorithm = algorithm_i32;
        assert_eq!(welcome_message.wrapper_algorithm, algorithm_i32);
    }
}
