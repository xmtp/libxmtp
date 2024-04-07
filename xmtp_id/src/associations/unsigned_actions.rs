use crate::associations::hashes::generate_inbox_id;

use super::MemberIdentifier;

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
        format!(
            // TODO: Finalize text
            "Create Inbox: {}",
            generate_inbox_id(&self.account_address, &self.nonce)
        )
    }
}

#[derive(Clone)]
pub struct UnsignedAddAssociation {
    pub inbox_id: String,
    pub new_member_identifier: MemberIdentifier,
}

impl SignatureTextCreator for UnsignedAddAssociation {
    fn signature_text(&self) -> String {
        format!(
            // TODO: Finalize text
            "Add {} to Inbox {}",
            self.new_member_identifier, self.inbox_id
        )
    }
}

#[derive(Clone)]
pub struct UnsignedRevokeAssociation {
    pub inbox_id: String,
    pub revoked_member: MemberIdentifier,
}

impl SignatureTextCreator for UnsignedRevokeAssociation {
    fn signature_text(&self) -> String {
        format!(
            // TODO: Finalize text
            "Remove {} from Inbox {}",
            self.revoked_member, self.inbox_id
        )
    }
}

#[derive(Clone)]
pub struct UnsignedChangeRecoveryAddress {
    pub inbox_id: String,
    pub new_recovery_address: String,
}

impl SignatureTextCreator for UnsignedChangeRecoveryAddress {
    fn signature_text(&self) -> String {
        format!(
            // TODO: Finalize text
            "Change Recovery Address for Inbox {} to {}",
            self.inbox_id, self.new_recovery_address
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
    pub client_timestamp_ns: u64,
    pub actions: Vec<UnsignedAction>,
}

impl UnsignedIdentityUpdate {
    pub fn new(client_timestamp_ns: u64, actions: Vec<UnsignedAction>) -> Self {
        UnsignedIdentityUpdate {
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
            "I authorize the following actions on XMTP:\n\n{}\n\nAuthorized at: {}",
            all_signatures.join("\n\n"),
            // TODO: Pretty up date
            self.client_timestamp_ns
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::associations::test_utils::{rand_string, rand_u64};

    use super::*;

    #[test]
    fn create_signatures() {
        let create_inbox = UnsignedCreateInbox {
            nonce: rand_u64(),
            account_address: rand_string(),
        };
        let inbox_id = generate_inbox_id(&create_inbox.account_address, &create_inbox.nonce);

        let add_association = UnsignedAddAssociation {
            inbox_id: inbox_id.clone(),
            new_member_identifier: MemberIdentifier::Address(rand_string()),
        };

        let revoke_association = UnsignedRevokeAssociation {
            inbox_id: inbox_id.clone(),
            revoked_member: MemberIdentifier::Address(rand_string()),
        };

        let change_recovery_address = UnsignedChangeRecoveryAddress {
            inbox_id: inbox_id.clone(),
            new_recovery_address: rand_string(),
        };

        let identity_update = UnsignedIdentityUpdate {
            client_timestamp_ns: rand_u64(),
            actions: vec![
                UnsignedAction::CreateInbox(create_inbox.clone()),
                UnsignedAction::AddAssociation(add_association.clone()),
                UnsignedAction::RevokeAssociation(revoke_association.clone()),
                UnsignedAction::ChangeRecoveryAddress(change_recovery_address.clone()),
            ],
        };

        let signature_text = identity_update.signature_text();
        let expected_text = format!("I authorize the following actions on XMTP:\n\nCreate Inbox: {}\n\nAdd {} to Inbox {}\n\nRemove {} from Inbox {}\n\nChange Recovery Address for Inbox {} to {}\n\nAuthorized at: {}",
        inbox_id,
        add_association.new_member_identifier,
        inbox_id,
        revoke_association.revoked_member,
        inbox_id,
        inbox_id,
        change_recovery_address.new_recovery_address,
        identity_update.client_timestamp_ns,
        );
        assert_eq!(signature_text, expected_text)
    }
}
