/// Std implementations for some generated types
use crate::xmtp::xmtpv4::envelopes::client_envelope::Payload;

impl std::fmt::Display for Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Payload::GroupMessage(_) => write!(f, "Payload::GroupMessage"),
            Payload::WelcomeMessage(_) => write!(f, "Payload::WelcomeMessage"),
            Payload::UploadKeyPackage(_) => write!(f, "Payload::UploadKeyPackage"),
            Payload::IdentityUpdate(_) => write!(f, "Payload::IdentityUpdate"),
        }
    }
}
