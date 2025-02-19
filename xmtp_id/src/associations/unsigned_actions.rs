use super::{member::RootIdentifier, MemberIdentifier};
use crate::associations::{member::HasMemberKind, MemberKind};
use chrono::DateTime;

const HEADER: &str = "XMTP : Authenticate to inbox";
const FOOTER: &str = "For more info: https://xmtp.org/signatures";

pub trait SignatureTextCreator {
    fn signature_text(&self) -> String;
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnsignedCreateInbox {
    pub nonce: u64,
    pub account_identifier: RootIdentifier,
}

impl SignatureTextCreator for UnsignedCreateInbox {
    fn signature_text(&self) -> String {
        format!("- Create inbox\n  (Owner: {:?})", self.account_identifier)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnsignedAddAssociation {
    pub new_member_identifier: MemberIdentifier,
}

impl SignatureTextCreator for UnsignedAddAssociation {
    fn signature_text(&self) -> String {
        let member_kind = self.new_member_identifier.kind();
        let id_kind = get_identifier_text(&member_kind);
        let prefix = match member_kind {
            MemberKind::Installation => "Grant messaging access to app",
            MemberKind::Ethereum => "Link address to inbox",
            MemberKind::Passkey => "Link passkey to inbox",
        };
        format!(
            "- {prefix}\n  ({id_kind}: {:?})",
            self.new_member_identifier
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnsignedRevokeAssociation {
    pub revoked_member: MemberIdentifier,
}

impl SignatureTextCreator for UnsignedRevokeAssociation {
    fn signature_text(&self) -> String {
        let member_kind = self.revoked_member.kind();
        let id_kind = get_identifier_text(&member_kind);
        let prefix = match self.revoked_member.kind() {
            MemberKind::Installation => "Revoke messaging access from app",
            MemberKind::Ethereum => "Unlink address from inbox",
            MemberKind::Passkey => "Unlink passkey from inbox",
        };
        format!("- {prefix}\n  ({id_kind}: {})", self.revoked_member)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnsignedChangeRecoveryAddress {
    pub new_recovery_identifier: RootIdentifier,
}

impl SignatureTextCreator for UnsignedChangeRecoveryAddress {
    fn signature_text(&self) -> String {
        format!(
            "- Change inbox recovery address\n  ({:?})",
            self.new_recovery_identifier
        )
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum UnsignedAction {
    CreateInbox(UnsignedCreateInbox),
    AddAssociation(UnsignedAddAssociation),
    RevokeAssociation(UnsignedRevokeAssociation),
    ChangeRecoveryAddress(UnsignedChangeRecoveryAddress),
}

impl SignatureTextCreator for UnsignedAction {
    fn signature_text(&self) -> String {
        match self {
            UnsignedAction::CreateInbox(action) => action.signature_text(),
            UnsignedAction::AddAssociation(action) => action.signature_text(),
            UnsignedAction::RevokeAssociation(action) => action.signature_text(),
            UnsignedAction::ChangeRecoveryAddress(action) => action.signature_text(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnsignedIdentityUpdate {
    pub inbox_id: String,
    pub client_timestamp_ns: u64,
    pub actions: Vec<UnsignedAction>,
}

impl UnsignedIdentityUpdate {
    pub fn new(actions: Vec<UnsignedAction>, inbox_id: String, client_timestamp_ns: u64) -> Self {
        UnsignedIdentityUpdate {
            inbox_id,
            client_timestamp_ns,
            actions,
        }
    }
}

impl SignatureTextCreator for UnsignedIdentityUpdate {
    fn signature_text(&self) -> String {
        let all_signatures = self
            .actions
            .iter()
            .map(|action| action.signature_text())
            .collect::<Vec<String>>();
        format!(
            "{HEADER}\n\nInbox ID: {}\nCurrent time: {}\n\n{}\n\n{FOOTER}",
            self.inbox_id,
            pretty_timestamp(self.client_timestamp_ns),
            all_signatures.join("\n"),
        )
    }
}

fn get_identifier_text(kind: &MemberKind) -> String {
    match kind {
        MemberKind::Ethereum => "Address".to_string(),
        MemberKind::Installation => "ID".to_string(),
        MemberKind::Passkey => "Passkey".to_string(),
    }
}

fn pretty_timestamp(ns_date: u64) -> String {
    let date = DateTime::from_timestamp_nanos(ns_date as i64);
    date.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn create_signatures() {
        let account_identifier = RootIdentifier::eth("0x1234567890abcdef1234567890abcdef12345678");

        let client_timestamp_ns: u64 = 12;
        let new_member_address = "0x4567890abcdef1234567890abcdef12345678123".to_string();
        let new_recovery_identifier =
            RootIdentifier::eth("0x7890abcdef1234567890abcdef12345678123456");
        let new_installation_id = vec![1, 2, 3];
        let create_inbox = UnsignedCreateInbox {
            nonce: 0,
            account_identifier: account_identifier.clone(),
        };
        let inbox_id = account_identifier.inbox_id(create_inbox.nonce).unwrap();

        let add_address = UnsignedAddAssociation {
            new_member_identifier: MemberIdentifier::new_ethereum(&new_member_address),
        };

        let add_installation = UnsignedAddAssociation {
            new_member_identifier: MemberIdentifier::new_installation(new_installation_id.clone()),
        };

        let revoke_address = UnsignedRevokeAssociation {
            revoked_member: MemberIdentifier::new_ethereum(new_member_address).into(),
        };

        let revoke_installation = UnsignedRevokeAssociation {
            revoked_member: MemberIdentifier::new_installation(new_installation_id.clone()),
        };

        let change_recovery_address = UnsignedChangeRecoveryAddress {
            new_recovery_identifier: new_recovery_identifier.clone(),
        };

        let identity_update = UnsignedIdentityUpdate {
            inbox_id: inbox_id.clone(),
            client_timestamp_ns,
            actions: vec![
                UnsignedAction::CreateInbox(create_inbox.clone()),
                UnsignedAction::AddAssociation(add_address.clone()),
                UnsignedAction::AddAssociation(add_installation.clone()),
                UnsignedAction::RevokeAssociation(revoke_address.clone()),
                UnsignedAction::RevokeAssociation(revoke_installation.clone()),
                UnsignedAction::ChangeRecoveryAddress(change_recovery_address.clone()),
            ],
        };
        let signature_text = identity_update.signature_text();
        let expected_text = "XMTP : Authenticate to inbox

Inbox ID: fcd18d86276d7a99fe522dba9660c420f03c8648785ada7c5daae232a3df77a9
Current time: 1970-01-01T00:00:00Z

- Create inbox
  (Owner: 0x1234567890abcdef1234567890abcdef12345678)
- Link address to inbox
  (Address: 0x4567890abcdef1234567890abcdef12345678123)
- Grant messaging access to app
  (ID: 010203)
- Unlink address from inbox
  (Address: 0x4567890abcdef1234567890abcdef12345678123)
- Revoke messaging access from app
  (ID: 010203)
- Change inbox recovery address
  (Address: 0x7890abcdef1234567890abcdef12345678123456)

For more info: https://xmtp.org/signatures";
        assert_eq!(signature_text, expected_text)
    }
}
