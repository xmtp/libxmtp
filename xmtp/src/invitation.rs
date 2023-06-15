use crate::{
    association::AssociationError,
    contact::{Contact, ContactError},
    session::SessionManager,
    vmac_protos::ProtoWrapper,
};
use prost::{DecodeError, EncodeError, Message};
use thiserror::Error;
use xmtp_proto::xmtp::v3::message_contents::{
    invitation_envelope::Version::V1 as V1Proto, InvitationEnvelope, InvitationEnvelopeV1,
    InvitationV1,
};

#[derive(Debug, Error)]
pub enum InvitationError {
    #[error("association error")]
    Association(#[from] AssociationError),
    #[error("contact error")]
    Contact(#[from] ContactError),
    #[error("bad data")]
    BadData(String),
    #[error("decode error")]
    Decode(#[from] DecodeError),
    #[error("encode error")]
    Encode(#[from] EncodeError),
    #[error("unknown error")]
    Unknown,
}

#[derive(Clone, Debug)]
pub struct Invitation {
    inviter: Contact,
    ciphertext: Vec<u8>,
}

impl Invitation {
    pub fn new(envelope: InvitationEnvelope) -> Result<Self, InvitationError> {
        let inviter = Self::inviter(envelope.clone())?;
        let ciphertext = Self::ciphertext(envelope.clone())?;
        let val = Self {
            inviter,
            ciphertext,
        };

        Ok(val)
    }

    pub fn build(
        inviter: Contact,
        mut session: SessionManager,
        inner_invite_bytes: Vec<u8>,
    ) -> Result<Invitation, InvitationError> {
        let encrypted = session.encrypt(inner_invite_bytes.as_slice());

        let envelope = InvitationEnvelope {
            version: Some(V1Proto(InvitationEnvelopeV1 {
                inviter: Some(inviter.bundle),
                ciphertext: encrypted.message().to_vec(),
            })),
        };

        Self::new(envelope)
    }

    pub fn build_inner_invite_bytes(
        invitee_wallet_address: String,
    ) -> Result<Vec<u8>, InvitationError> {
        let inner_invite = InvitationV1 {
            invitee_wallet_address,
        };

        let invite_bytes: Vec<u8> = ProtoWrapper {
            proto: inner_invite,
        }
        .try_into()?;

        Ok(invite_bytes)
    }

    pub fn build_proto(&self) -> InvitationEnvelope {
        InvitationEnvelope {
            version: Some(V1Proto(InvitationEnvelopeV1 {
                inviter: Some(self.inviter.bundle.clone()),
                ciphertext: self.ciphertext.clone(),
            })),
        }
    }

    fn ciphertext(envelope: InvitationEnvelope) -> Result<Vec<u8>, InvitationError> {
        let ciphertext = match envelope.version {
            Some(V1Proto(env)) => env.ciphertext,
            None => return Err(InvitationError::BadData("no version".to_string())),
        };

        Ok(ciphertext)
    }

    fn inviter(envelope: InvitationEnvelope) -> Result<Contact, InvitationError> {
        let env = match envelope.version {
            Some(V1Proto(env)) => Contact::new(env.inviter.unwrap(), None)?,
            None => return Err(InvitationError::BadData("no version".to_string())),
        };

        Ok(env)
    }
}

impl From<Invitation> for InvitationEnvelope {
    fn from(invitation: Invitation) -> Self {
        invitation.build_proto()
    }
}

impl TryFrom<Vec<u8>> for Invitation {
    type Error = InvitationError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let envelope = InvitationEnvelope::decode(value.as_slice())?;
        let invitation = Self::new(envelope)?;

        Ok(invitation)
    }
}

impl TryFrom<Invitation> for Vec<u8> {
    type Error = InvitationError;

    fn try_from(value: Invitation) -> Result<Self, Self::Error> {
        let mut buf = Vec::new();
        value.build_proto().encode(&mut buf)?;

        Ok(buf)
    }
}

impl TryFrom<ProtoWrapper<InvitationV1>> for Vec<u8> {
    type Error = InvitationError;

    fn try_from(invitation: ProtoWrapper<InvitationV1>) -> Result<Self, Self::Error> {
        let mut buf = Vec::new();
        invitation.proto.encode(&mut buf)?;

        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use xmtp_proto::xmtp::v3::message_contents::VmacInstallationPublicKeyBundleV1;
    use xmtp_proto::xmtp::v3::message_contents::{
        installation_contact_bundle::Version as ContactBundleVersionProto,
        InstallationContactBundle,
    };

    use crate::account::{tests::test_wallet_signer, Account};
    use crate::contact::Contact;
    use crate::ClientBuilder;

    use super::Invitation;

    #[test]
    fn serialize_round_trip() {
        let mut client = ClientBuilder::new_test().build().unwrap();
        let other_account = Account::generate(test_wallet_signer).unwrap();
        let session = client
            .create_outbound_session(other_account.contact())
            .unwrap();

        let invitation = Invitation::build(
            client.account.contact(),
            session,
            Invitation::build_inner_invite_bytes(other_account.addr().to_string()).unwrap(),
        )
        .unwrap();

        assert_eq!(invitation.inviter.id(), client.account.contact().id());

        let bytes: Vec<u8> = invitation.clone().try_into().unwrap();
        let invitation2: Invitation = bytes.try_into().unwrap();

        assert_eq!(invitation2.inviter.id(), invitation.inviter.id());

        assert_eq!(invitation.ciphertext, invitation2.ciphertext);
    }

    #[test]
    fn fail_on_bad_invite() {
        let client = ClientBuilder::new_test().build().unwrap();
        let other_account = Account::generate(test_wallet_signer).unwrap();
        let session = client
            .create_outbound_session(other_account.contact())
            .unwrap();

        let bad_bundle = InstallationContactBundle {
            version: Some(ContactBundleVersionProto::V1(
                VmacInstallationPublicKeyBundleV1 {
                    identity_key: None,
                    fallback_key: None,
                },
            )),
        };

        let bad_invite = Invitation::build(
            Contact { bundle: bad_bundle },
            session,
            Invitation::build_inner_invite_bytes(other_account.addr().to_string()).unwrap(),
        );

        assert!(bad_invite.is_err());
    }
}
