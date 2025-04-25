use super::xmtp::identity::associations::*;
use super::xmtp::mls::api::v1::*;
use crate::xmtp::xmtpv4::envelopes::client_envelope;

// Debug implementation for client_envelope::Payload enum
impl std::fmt::Display for client_envelope::Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GroupMessage(msg) => f
                .debug_struct("GroupMessage")
                .field("version", &msg.version)
                .finish(),
            Self::WelcomeMessage(msg) => f
                .debug_struct("WelcomeMessage")
                .field("version", &msg.version)
                .finish(),
            Self::UploadKeyPackage(req) => f
                .debug_struct("UploadKeyPackage")
                .field("key_package", &req.key_package)
                .field("is_inbox_id_credential", &req.is_inbox_id_credential)
                .finish(),
            Self::IdentityUpdate(update) => f
                .debug_struct("IdentityUpdate")
                .field("inbox_id", &update.inbox_id)
                .field("client_timestamp_ns", &update.client_timestamp_ns)
                .field("actions", &update.actions)
                .finish(),
        }
    }
}

impl std::fmt::Display for IdentityUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentityUpdate")
            .field("inbox_id", &self.inbox_id)
            .field("client_timestamp_ns", &self.client_timestamp_ns)
            .field(
                "actions",
                &self
                    .actions
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

// Debug implementation for IdentityAction
impl std::fmt::Display for IdentityAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(kind) = &self.kind {
            match kind {
                identity_action::Kind::CreateInbox(create) => f
                    .debug_struct("IdentityAction::CreateInbox")
                    .field("initial_identifier", &create.initial_identifier)
                    .field("nonce", &create.nonce)
                    .field(
                        "initial_identifier_kind",
                        &format_identifier_kind(create.initial_identifier_kind),
                    )
                    .field("relying_party", &create.relying_party)
                    .finish(),
                identity_action::Kind::Add(add) => f
                    .debug_struct("IdentityAction::AddAssociation")
                    .field(
                        "new_member",
                        &add.new_member_identifier.as_ref().map(|s| s.to_string()),
                    )
                    .field("relying_party", &add.relying_party)
                    .finish(),
                identity_action::Kind::Revoke(revoke) => f
                    .debug_struct("IdentityAction::RevokeAssociation")
                    .field(
                        "member_to_revoke",
                        &revoke.member_to_revoke.as_ref().map(|s| s.to_string()),
                    )
                    .finish(),
                identity_action::Kind::ChangeRecoveryAddress(change) => f
                    .debug_struct("IdentityAction::ChangeRecoveryAddress")
                    .field("new_recovery_identifier", &change.new_recovery_identifier)
                    .field(
                        "new_recovery_identifier_kind",
                        &format_identifier_kind(change.new_recovery_identifier_kind),
                    )
                    .field("relying_party", &change.relying_party)
                    .finish(),
            }
        } else {
            f.debug_struct("IdentityAction")
                .field("kind", &"None")
                .finish()
        }
    }
}

// Debug implementation for group_message::Version
impl std::fmt::Display for group_message::Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V1(v1) => f
                .debug_struct("V1")
                .field("id", &v1.id)
                .field("created_ns", &v1.created_ns)
                .field("group_id", &format_bytes(&v1.group_id))
                .field("data", &format_bytes(&v1.data))
                .field("sender_hmac", &format_bytes(&v1.sender_hmac))
                .field("should_push", &v1.should_push)
                .finish(),
        }
    }
}

// Debug implementation for welcome_message::Version
impl std::fmt::Display for welcome_message::Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V1(v1) => f
                .debug_struct("V1")
                .field("id", &v1.id)
                .field("created_ns", &v1.created_ns)
                .field("installation_key", &format_bytes(&v1.installation_key))
                .field("data", &format_bytes(&v1.data))
                .field("hpke_public_key", &format_bytes(&v1.hpke_public_key))
                .finish(),
        }
    }
}

// Debug implementation for group_message_input::Version
impl std::fmt::Display for group_message_input::Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V1(v1) => f
                .debug_struct("V1")
                .field("data", &format_bytes(&v1.data))
                .field("sender_hmac", &format_bytes(&v1.sender_hmac))
                .field("should_push", &v1.should_push)
                .finish(),
        }
    }
}

// Debug implementation for welcome_message_input::Version
impl std::fmt::Display for welcome_message_input::Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V1(v1) => f
                .debug_struct("V1")
                .field("installation_key", &format_bytes(&v1.installation_key))
                .field("data", &format_bytes(&v1.data))
                .field("hpke_public_key", &format_bytes(&v1.hpke_public_key))
                .finish(),
        }
    }
}

// Debug implementation for GroupMessageInput
impl std::fmt::Display for GroupMessageInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupMessageInput")
            .field("version", &self.version)
            .finish()
    }
}

// Debug implementation for WelcomeMessageInput
impl std::fmt::Display for WelcomeMessageInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WelcomeMessageInput")
            .field("version", &self.version)
            .finish()
    }
}

// Debug implementation for KeyPackageUpload
impl std::fmt::Display for KeyPackageUpload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyPackageUpload")
            .field(
                "key_package_tls_serialized",
                &format_bytes(&self.key_package_tls_serialized),
            )
            .finish()
    }
}

// Debug implementation for identity_action::Kind
impl std::fmt::Display for identity_action::Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateInbox(create) => f
                .debug_struct("CreateInbox")
                .field("nonce", &create.nonce)
                .field("initial_identifier", &create.initial_identifier)
                .finish(),
            Self::Add(add) => f
                .debug_struct("Add")
                .field("new_member_identifier", &add.new_member_identifier)
                .finish(),
            Self::Revoke(revoke) => f
                .debug_struct("Revoke")
                .field("member_to_revoke", &revoke.member_to_revoke)
                .finish(),
            Self::ChangeRecoveryAddress(change) => f
                .debug_struct("ChangeRecoveryAddress")
                .field("new_recovery_identifier", &change.new_recovery_identifier)
                .field(
                    "new_recovery_identifier_kind",
                    &change.new_recovery_identifier_kind,
                )
                .finish(),
        }
    }
}

// Debug implementation for MemberIdentifier
impl std::fmt::Display for MemberIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(kind) = &self.kind {
            match kind {
                member_identifier::Kind::EthereumAddress(addr) => f
                    .debug_struct("EthereumAddress")
                    .field("address", addr)
                    .finish(),
                member_identifier::Kind::InstallationPublicKey(key) => f
                    .debug_struct("InstallationPublicKey")
                    .field("key", &format_bytes(key))
                    .finish(),
                member_identifier::Kind::Passkey(pk) => f
                    .debug_struct("Passkey")
                    .field("key", &format_bytes(&pk.key))
                    .finish(),
            }
        } else {
            f.debug_struct("MemberIdentifier")
                .field("kind", &"None")
                .finish()
        }
    }
}

// Helper for hex formatting
fn format_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "[]".to_string();
    }

    let len = bytes.len();
    if len <= 8 {
        format!(
            "[{}]",
            bytes
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" ")
        )
    } else {
        format!(
            "[{} {} ... {} {}] ({} bytes)",
            format!("{:02x}", bytes[0]),
            format!("{:02x}", bytes[1]),
            format!("{:02x}", bytes[len - 2]),
            format!("{:02x}", bytes[len - 1]),
            len
        )
    }
}

// Helper function to format IdentifierKind
fn format_identifier_kind(kind: i32) -> String {
    match kind {
        0 => "Unspecified".to_string(),
        1 => "Ethereum".to_string(),
        2 => "Passkey".to_string(),
        _ => format!("Unknown({})", kind),
    }
}
