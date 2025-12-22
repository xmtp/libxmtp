mod update_dedupe;

use crate::types::GlobalCursor;
/// implementations for some generated types
use crate::xmtp::mls::api::v1::welcome_message::Version;
use crate::xmtp::mls::message_contents::{
    GroupUpdated, WelcomePointeeEncryptionAeadType, WelcomePointeeEncryptionAeadTypesExtension,
};
use crate::xmtp::xmtpv4::envelopes::AuthenticatedData;
use crate::xmtp::xmtpv4::envelopes::client_envelope::Payload;
use std::hash::Hash;

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

impl std::fmt::Display for AuthenticatedData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(d) = &self.depends_on {
            let cursor: GlobalCursor = d.clone().into();
            write!(f, "aad[{} -> {}]", hex::encode(&self.target_topic), cursor)?;
        } else {
            write!(
                f,
                "aad[{} -> (no dependency)]",
                hex::encode(&self.target_topic)
            )?;
        }
        Ok(())
    }
}

impl Hash for GroupUpdated {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.initiated_by_inbox_id.hash(state);
        self.added_inboxes.hash(state);
        self.removed_inboxes.hash(state);
        self.metadata_field_changes.hash(state);
        self.left_inboxes.hash(state);
        self.added_admin_inboxes.hash(state);
        self.removed_admin_inboxes.hash(state);
        self.added_super_admin_inboxes.hash(state);
        self.removed_super_admin_inboxes.hash(state);
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
            Version::WelcomePointer(w) => w.installation_key.as_slice(),
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
