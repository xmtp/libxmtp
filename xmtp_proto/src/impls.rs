use std::fmt::Display;

/// Std implementations for some generated types
use crate::{
    mls_v1::{GroupMessageInput, UploadKeyPackageRequest, WelcomeMessageInput},
    xmtp::{
        identity::associations::{
            identity_action, member_identifier::Kind, IdentityAction, IdentityUpdate,
            MemberIdentifier,
        },
        xmtpv4::envelopes::{client_envelope::Payload, AuthenticatedData, ClientEnvelope},
    },
};
use xmtp_common::fmt;

impl Display for Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Payload::GroupMessage(m) => write!(f, "Payload::GroupMessage [{}]", m),
            Payload::WelcomeMessage(m) => write!(f, "Payload::WelcomeMessage [{}]", m),
            Payload::UploadKeyPackage(m) => write!(f, "Payload::UploadKeyPackage [{}]", m),
            Payload::IdentityUpdate(m) => write!(f, "Payload::IdentityUpdate [{}]", m),
        }
    }
}

impl Display for GroupMessageInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::mls_v1::group_message_input::Version;
        if let Some(version) = &self.version {
            match version {
                Version::V1(v1) => {
                    write!(f, ", data [{}]", fmt::truncate_hex(hex::encode(&v1.data)))?;
                    write!(
                        f,
                        ", hmac [{}]",
                        fmt::truncate_hex(hex::encode(&v1.sender_hmac))
                    )?;
                    write!(f, ", should_push [{}]", v1.should_push)?;
                }
            }
        }
        Ok(())
    }
}

impl Display for WelcomeMessageInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::mls_v1::welcome_message_input::Version;
        if let Some(version) = &self.version {
            match version {
                Version::V1(v1) => {
                    write!(f, "[V1]")?;
                    write!(
                        f,
                        "installation_key [{}]",
                        hex::encode(&v1.installation_key)
                    )?;
                    write!(f, "data [{}]", fmt::truncate_hex(hex::encode(&v1.data)))?;
                    write!(
                        f,
                        "hpke_public_key [{}]",
                        fmt::truncate_hex(hex::encode(&v1.hpke_public_key))
                    )?;
                }
            }
        }
        Ok(())
    }
}

impl Display for UploadKeyPackageRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UploadKeyPackageRequest {{")?;
        if let Some(kp) = &self.key_package {
            write!(
                f,
                "key_package [{}]",
                fmt::truncate_hex(hex::encode(&kp.key_package_tls_serialized))
            )?;
        }
        write!(
            f,
            "is_inbox_id_credential [{}]",
            self.is_inbox_id_credential
        )?;
        write!(f, "}}")?;
        Ok(())
    }
}

impl Display for IdentityUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IdentityUpdate {{")?;
        write!(f, "client_timestamp [{}]", self.client_timestamp_ns)?;
        write!(f, ", inbox_id [{}]", self.inbox_id)?;
        write!(f, "}} ")?;
        for action in self.actions.iter() {
            write!(f, "{}", action)?;
        }
        Ok(())
    }
}

impl Display for IdentityAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(k) = &self.kind {
            write!(f, "IdentityAction {{ {} }}", k)?;
        }
        Ok(())
    }
}

impl Display for identity_action::Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use identity_action::Kind::*;
        match self {
            CreateInbox(c) => {
                write!(f, "CreateInbox {{")?;
                write!(f, ", initial_identifier [{}]", c.initial_identifier)?;
                write!(f, ", nonce [{}]", c.nonce)?;
                write!(
                    f,
                    ", initial_identifier_signature is_some? [{}]",
                    c.initial_identifier_signature.is_some()
                )?;
                write!(f, ", relying_party [{:?}]", c.relying_party)?;
                write!(f, "}}")?;
            }
            Add(a) => {
                write!(f, "AddAssociation {{")?;
                if let Some(m) = &a.new_member_identifier {
                    write!(f, "new_member_identifier [{}]", m)?;
                }
                write!(
                    f,
                    ", existing_member_signature is_some? [{}]",
                    a.existing_member_signature.is_some()
                )?;
                write!(
                    f,
                    ", new_member_signature is_some? [{}]",
                    a.new_member_signature.is_some()
                )?;
                write!(f, ", relying_party [{:?}]", a.relying_party)?;
            }
            Revoke(r) => {
                write!(f, "Revoke {{")?;
                if let Some(r) = &r.member_to_revoke {
                    write!(f, ", member_to_revoke {}", r)?;
                }
                write!(
                    f,
                    ", recovery_identifier_signature is_some? {}",
                    r.recovery_identifier_signature.is_some()
                )?;
                write!(f, "}}")?;
            }
            ChangeRecoveryAddress(r) => {
                write!(f, "ChangeRecoveryAddress {{")?;
                write!(
                    f,
                    ", new_recovery_identifier [{}]",
                    r.new_recovery_identifier
                )?;
                write!(
                    f,
                    ", existing_recover_identifier_signature is_some? [{}]",
                    r.existing_recovery_identifier_signature.is_some()
                )?;
                write!(
                    f,
                    " new_recovery_identifier_kind [{}]",
                    r.new_recovery_identifier_kind
                )?;
                write!(f, " relying_party [{:?}]", r.relying_party)?;
                write!(f, " }}")?;
            }
        }
        Ok(())
    }
}

impl Display for MemberIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(k) = &self.kind {
            match k {
                Kind::EthereumAddress(e) => write!(f, "EthKey [{}]", e)?,
                Kind::InstallationPublicKey(p) => {
                    write!(f, "Installation Key [{}]", hex::encode(&p))?
                }
                Kind::Passkey(p) => {
                    write!(f, "Passkey {{")?;
                    write!(f, " key [{}]", hex::encode(&p.key))?;
                    write!(f, " relying_party [{:?}]", p.relying_party)?;
                }
            }
        }
        Ok(())
    }
}

impl Display for AuthenticatedData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AuthenticatedData {{")?;
        #[allow(deprecated)]
        write!(f, "  target_originator [{:?}]", self.target_originator)?;
        write!(f, "  depends_on CURSOR")?;
        write!(f, "  is_commit [{}]", self.is_commit)?;
        write!(f, "}}")?;
        Ok(())
    }
}

impl Display for ClientEnvelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ClientEnvelope {{")?;
        if let Some(a) = &self.aad {
            write!(f, "   aad [{}]", a)?;
        }
        if let Some(p) = &self.payload {
            write!(f, "    payload [{}]", p)?;
        }
        write!(f, "}}")?;
        Ok(())
    }
}
