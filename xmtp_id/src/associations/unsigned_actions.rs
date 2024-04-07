use chrono::DateTime;

use crate::associations::MemberKind;

use super::MemberIdentifier;

const HEADER: &str = "XMTP : Authenticate to inbox";
const FOOTER: &str = "For more info: https://xmtp.org/signatures";

pub trait SignatureTextCreator {
    fn signature_text(&self) -> String;
}

#[derive(Clone)]
pub struct UnsignedCreateInbox {
    pub nonce: u64,
    pub account_address: String,
}

impl SignatureTextCreator for UnsignedCreateInbox {
    fn signature_text(&self) -> String {
        format!("- Create inbox\n(Owner: {})", self.account_address)
    }
}

#[derive(Clone)]
pub struct UnsignedAddAssociation {
    pub new_member_identifier: MemberIdentifier,
}

impl SignatureTextCreator for UnsignedAddAssociation {
    fn signature_text(&self) -> String {
        let member_kind = self.new_member_identifier.kind();
        let id_kind = get_identifier_text(&member_kind);
        let prefix = match member_kind {
            MemberKind::Installation => "Grant messaging access to app",
            MemberKind::Address => "Link address to inbox",
        };
        format!("- {prefix}\n({id_kind}: {})", self.new_member_identifier)
    }
}

#[derive(Clone)]
pub struct UnsignedRevokeAssociation {
    pub revoked_member: MemberIdentifier,
}

impl SignatureTextCreator for UnsignedRevokeAssociation {
    fn signature_text(&self) -> String {
        let member_kind = self.revoked_member.kind();
        let id_kind = get_identifier_text(&member_kind);
        let prefix = match self.revoked_member.kind() {
            MemberKind::Installation => "Revoke messaging access from app",
            MemberKind::Address => "Unlink address from inbox",
        };
        format!("- {prefix}\n({id_kind}: {})", self.revoked_member)
    }
}

#[derive(Clone)]
pub struct UnsignedChangeRecoveryAddress {
    pub new_recovery_address: String,
}

impl SignatureTextCreator for UnsignedChangeRecoveryAddress {
    fn signature_text(&self) -> String {
        format!(
            // TODO: Finalize text
            "- Change inbox recovery address\n(Address: {})",
            self.new_recovery_address
        )
    }
}

#[allow(dead_code)]
#[derive(Clone)]
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

#[derive(Clone)]
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
            ns_date_to_iso_8601(self.client_timestamp_ns),
            all_signatures.join("\n"),
        )
    }
}

fn get_identifier_text(kind: &MemberKind) -> String {
    match kind {
        MemberKind::Address => "Address".to_string(),
        MemberKind::Installation => "ID".to_string(),
    }
}

fn ns_date_to_iso_8601(ns_date: u64) -> String {
    let date = DateTime::from_timestamp_nanos(ns_date as i64);
    date.to_string()
}

#[cfg(test)]
mod tests {
    use crate::associations::{
        hashes::generate_inbox_id,
        test_utils::{rand_string, rand_u64},
    };

    use super::*;

    #[test]
    fn create_signatures() {
        let account_address = rand_string();
        let client_timestamp_ns = rand_u64();
        let create_inbox = UnsignedCreateInbox {
            nonce: rand_u64(),
            account_address: account_address.clone(),
        };
        let inbox_id = generate_inbox_id(&create_inbox.account_address, &create_inbox.nonce);

        let add_association = UnsignedAddAssociation {
            new_member_identifier: MemberIdentifier::Address(rand_string()),
        };

        let revoke_association = UnsignedRevokeAssociation {
            revoked_member: MemberIdentifier::Address(rand_string()),
        };

        let change_recovery_address = UnsignedChangeRecoveryAddress {
            new_recovery_address: rand_string(),
        };

        let identity_update = UnsignedIdentityUpdate {
            inbox_id: inbox_id.clone(),
            client_timestamp_ns: client_timestamp_ns.clone(),
            actions: vec![
                UnsignedAction::CreateInbox(create_inbox.clone()),
                UnsignedAction::AddAssociation(add_association.clone()),
                UnsignedAction::RevokeAssociation(revoke_association.clone()),
                UnsignedAction::ChangeRecoveryAddress(change_recovery_address.clone()),
            ],
        };
        let now_timestamp = ns_date_to_iso_8601(client_timestamp_ns);
        let signature_text = identity_update.signature_text();
        let expected_text = format!(
            "{HEADER}

Inbox ID: {inbox_id}
Current time: {now_timestamp}

- Create inbox
(Owner: {})
- Link address to inbox
(Address: {})
- Unlink address from inbox
(Address: {})
- Change inbox recovery address
(Address: {})

{FOOTER}",
            account_address,
            add_association.new_member_identifier,
            revoke_association.revoked_member,
            change_recovery_address.new_recovery_address,
        );
        assert_eq!(signature_text, expected_text)
    }
}
