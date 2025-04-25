use super::unverified::*;

// UnverifiedIdentityUpdate Debug implementation
impl std::fmt::Debug for UnverifiedIdentityUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnverifiedIdentityUpdate")
            .field("inbox_id", &self.inbox_id)
            .field(
                "client_timestamp_ns",
                &format!("{} ns", self.client_timestamp_ns),
            )
            .field("actions", &format!("{} actions", self.actions.len()))
            .field("actions_detail", &self.actions)
            .finish()
    }
}

// UnverifiedAction Debug implementation
impl std::fmt::Debug for UnverifiedAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateInbox(create) => f
                .debug_struct("CreateInbox")
                .field("account_id", &create.unsigned_action.account_identifier)
                .field("nonce", &create.unsigned_action.nonce)
                .field(
                    "signature_type",
                    &signature_type_name(&create.initial_identifier_signature),
                )
                .finish(),
            Self::AddAssociation(add) => f
                .debug_struct("AddAssociation")
                .field("new_member", &add.unsigned_action.new_member_identifier)
                .field(
                    "new_sig_type",
                    &signature_type_name(&add.new_member_signature),
                )
                .field(
                    "existing_sig_type",
                    &signature_type_name(&add.existing_member_signature),
                )
                .finish(),
            Self::RevokeAssociation(revoke) => f
                .debug_struct("RevokeAssociation")
                .field("revoked_member", &revoke.unsigned_action.revoked_member)
                .field(
                    "recovery_sig_type",
                    &signature_type_name(&revoke.recovery_identifier_signature),
                )
                .finish(),
            Self::ChangeRecoveryAddress(change) => f
                .debug_struct("ChangeRecoveryAddress")
                .field(
                    "new_recovery",
                    &change.unsigned_action.new_recovery_identifier,
                )
                .field(
                    "recovery_sig_type",
                    &signature_type_name(&change.recovery_identifier_signature),
                )
                .finish(),
        }
    }
}

// Helper function to get signature type name
fn signature_type_name(sig: &UnverifiedSignature) -> &'static str {
    match sig {
        UnverifiedSignature::InstallationKey(_) => "InstallationKey",
        UnverifiedSignature::RecoverableEcdsa(_) => "RecoverableEcdsa",
        UnverifiedSignature::SmartContractWallet(_) => "SmartContractWallet",
        UnverifiedSignature::LegacyDelegated(_) => "LegacyDelegated",
        UnverifiedSignature::Passkey(_) => "Passkey",
    }
}

// UnverifiedSignature Debug implementation
impl std::fmt::Debug for UnverifiedSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InstallationKey(sig) => f
                .debug_struct("InstallationKey")
                .field("sig_bytes_len", &sig.signature_bytes.len())
                .field("key_bytes", &format_hex(&sig.verifying_key_bytes()))
                .finish(),
            Self::RecoverableEcdsa(sig) => f
                .debug_struct("RecoverableEcdsa")
                .field("sig_bytes_len", &sig.signature_bytes.len())
                .field("sig_preview", &format_hex_preview(&sig.signature_bytes))
                .finish(),
            Self::SmartContractWallet(sig) => f
                .debug_struct("SmartContractWallet")
                .field("account_id", &sig.account_id)
                .field("block_number", &sig.block_number)
                .field("sig_bytes_len", &sig.signature_bytes.len())
                .field("sig_preview", &format_hex_preview(&sig.signature_bytes))
                .finish(),
            Self::LegacyDelegated(sig) => f
                .debug_struct("LegacyDelegated")
                .field(
                    "key_sig_len",
                    &sig.legacy_key_signature.signature_bytes.len(),
                )
                .field("proto_bytes", &"[SignedPublicKeyProto]")
                .finish(),
            Self::Passkey(sig) => f
                .debug_struct("Passkey")
                .field("pub_key_len", &sig.public_key.len())
                .field("sig_len", &sig.signature.len())
                .field("auth_data_len", &sig.authenticator_data.len())
                .field("client_data_len", &sig.client_data_json.len())
                .finish(),
        }
    }
}

// Helper function to format byte slices for display
fn format_hex_preview(bytes: &[u8]) -> String {
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

// Helper function to format a full byte slice
fn format_hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "[]".to_string();
    }

    format!(
        "[{}]",
        bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

// Additional Debug implementations for remaining structs
impl std::fmt::Debug for UnverifiedCreateInbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnverifiedCreateInbox")
            .field("account_id", &self.unsigned_action.account_identifier)
            .field("nonce", &self.unsigned_action.nonce)
            .field("signature", &self.initial_identifier_signature)
            .finish()
    }
}

impl std::fmt::Debug for UnverifiedAddAssociation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnverifiedAddAssociation")
            .field("new_member", &self.unsigned_action.new_member_identifier)
            .field("new_sig", &self.new_member_signature)
            .field("existing_sig", &self.existing_member_signature)
            .finish()
    }
}

impl std::fmt::Debug for UnverifiedRevokeAssociation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnverifiedRevokeAssociation")
            .field("revoked_member", &self.unsigned_action.revoked_member)
            .field("recovery_sig", &self.recovery_identifier_signature)
            .finish()
    }
}

impl std::fmt::Debug for UnverifiedChangeRecoveryAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnverifiedChangeRecoveryAddress")
            .field(
                "new_recovery",
                &self.unsigned_action.new_recovery_identifier,
            )
            .field("recovery_sig", &self.recovery_identifier_signature)
            .finish()
    }
}

impl std::fmt::Debug for UnverifiedInstallationKeySignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnverifiedInstallationKeySignature")
            .field("sig_bytes", &format_hex_preview(&self.signature_bytes))
            .field(
                "key_bytes",
                &format_hex_preview(&self.verifying_key.as_ref().as_ref()),
            )
            .finish()
    }
}

impl std::fmt::Debug for UnverifiedPasskeySignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnverifiedPasskeySignature")
            .field("public_key", &format_hex_preview(&self.public_key))
            .field("signature", &format_hex_preview(&self.signature))
            .field("auth_data", &format_hex_preview(&self.authenticator_data))
            .field("client_json", &format_json_preview(&self.client_data_json))
            .finish()
    }
}

// Helper to show a preview of JSON content
fn format_json_preview(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) if s.len() <= 30 => format!("{:?}", s),
        Ok(s) => format!("{:?}... ({} bytes)", &s[..30], bytes.len()),
        Err(_) => format!("[binary data] ({} bytes)", bytes.len()),
    }
}

impl std::fmt::Debug for UnverifiedRecoverableEcdsaSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnverifiedRecoverableEcdsaSignature")
            .field("signature", &format_hex_preview(&self.signature_bytes))
            .finish()
    }
}

impl std::fmt::Debug for UnverifiedSmartContractWalletSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnverifiedSmartContractWalletSignature")
            .field("account_id", &self.account_id)
            .field("block", &self.block_number)
            .field("signature", &format_hex_preview(&self.signature_bytes))
            .finish()
    }
}

impl std::fmt::Debug for UnverifiedLegacyDelegatedSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnverifiedLegacyDelegatedSignature")
            .field("legacy_signature", &self.legacy_key_signature)
            .field("proto", &"[SignedPublicKeyProto]")
            .finish()
    }
}
