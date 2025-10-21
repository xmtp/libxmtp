/// Std implementations for some generated types
use crate::xmtp::xmtpv4::envelopes::client_envelope::Payload;

impl std::fmt::Display for Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Payload::GroupMessage(_) => write!(f, "Payload::GroupMessage"),
            Payload::WelcomeMessage(_) => write!(f, "Payload::WelcomeMessage"),
            Payload::UploadKeyPackage(_) => write!(f, "Payload::UploadKeyPackage"),
            Payload::IdentityUpdate(_) => write!(f, "Payload::IdentityUpdate"),
            Payload::PayerReport(_) => write!(f, "Payload::PayerReport"),
            Payload::PayerReportAttestation(_) => write!(f, "Payload::PayerReportAttestation"),
        }
    }
}

xmtp_common::if_test! {
    use crate::mls_v1::{group_message, welcome_message};
    use xmtp_common::Generate;

    impl Generate for welcome_message::V1 {
        fn generate() -> Self {
            welcome_message::V1 {
                id: xmtp_common::rand_u64(),
                created_ns: xmtp_common::rand_u64(),
                installation_key: xmtp_common::rand_vec::<32>(),
                data: xmtp_common::rand_vec::<6>(),
                hpke_public_key: xmtp_common::rand_vec::<6>(),
                wrapper_algorithm: 1,
                welcome_metadata: xmtp_common::rand_vec::<12>(),
            }
        }
    }

    impl Generate for group_message::V1 {
        fn generate() -> Self {
            group_message::V1 {
                id: xmtp_common::rand_u64(),
                created_ns: xmtp_common::rand_u64(),
                group_id: xmtp_common::rand_vec::<16>(),
                data: xmtp_common::rand_vec::<6>(),
                sender_hmac: xmtp_common::rand_vec::<6>(),
                should_push: false,
                is_commit: false,
            }
        }
    }
}
