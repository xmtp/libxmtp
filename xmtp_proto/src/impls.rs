/// implementations for some generated types
use crate::xmtp::mls::api::v1::welcome_message::Version;
use crate::xmtp::mls::message_contents::{
    WelcomePointeeEncryptionAeadType, WelcomePointeeEncryptionAeadTypesExtension,
};
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

impl Version {
    pub fn id(&self) -> u64 {
        match self {
            Version::V1(v1) => v1.id,
            Version::WelcomePointer(w) => w.id,
        }
    }
    pub fn created_ns(&self) -> u64 {
        match self {
            Version::V1(v1) => v1.created_ns,
            Version::WelcomePointer(w) => w.created_ns,
        }
    }
    pub fn installation_key(&self) -> &[u8] {
        match self {
            Version::V1(v1) => v1.installation_key.as_slice(),
            Version::WelcomePointer(w) => w.destination.as_slice(),
        }
    }
}

impl WelcomePointeeEncryptionAeadTypesExtension {
    pub fn available_types() -> Self {
        Self {
            supported_aead_types: vec![WelcomePointeeEncryptionAeadType::Chacha20Poly1305.into()],
        }
    }
}
