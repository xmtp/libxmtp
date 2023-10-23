use prost::{DecodeError, EncodeError, Message};
use thiserror::Error;

use vodozemac::Curve25519PublicKey;
use xmtp_proto::xmtp::v3::message_contents::{
    installation_contact_bundle::Version as ContactBundleVersionProto,
    vmac_account_linked_key::Association as AssociationProto, vmac_unsigned_public_key,
    Eip191Association as Eip191AssociationProto, InstallationContactBundle, VmacAccountLinkedKey,
};

use crate::{
    association::{AssociationError, Eip191Association},
    utils::key_fingerprint,
    vmac_protos::ProtoWrapper,
};

#[derive(Debug, Error)]
pub enum ContactError {
    #[error("association error")]
    Association(#[from] AssociationError),
    #[error("bad data")]
    BadData,
    #[error("decode error")]
    Decode(#[from] DecodeError),
    #[error("encode error")]
    Encode(#[from] EncodeError),
}

#[derive(Clone, Debug)]
pub struct Contact {
    pub(crate) bundle: InstallationContactBundle,
    pub wallet_address: String,
}

impl Contact {
    pub fn new(
        bundle: InstallationContactBundle,
        wallet_address: String,
    ) -> Result<Self, ContactError> {
        let contact = Self {
            bundle,
            wallet_address,
        };
        // .association() will return an error if it fails to validate
        // If you try and create with a wallet address that doesn't match the signature, this will fail
        contact.association()?;

        Ok(contact)
    }

    pub fn from_unknown_wallet(bundle: InstallationContactBundle) -> Result<Self, ContactError> {
        let ik = extract_identity_key(bundle.clone())?;
        let association = extract_proto_association(ik)?;

        Self::new(bundle, association.wallet_address)
    }

    pub fn from_bytes(
        bytes: Vec<u8>,
        expected_wallet_address: String,
    ) -> Result<Self, ContactError> {
        let bundle = InstallationContactBundle::decode(bytes.as_slice())?;

        Self::new(bundle, expected_wallet_address)
    }

    pub fn identity_key(&self) -> Result<VmacAccountLinkedKey, ContactError> {
        extract_identity_key(self.bundle.clone())
    }

    pub fn association(&self) -> Result<Eip191Association, ContactError> {
        let ik = self.identity_key()?;
        let key_bytes = match ik.clone().key {
            Some(key) => match key.union {
                Some(vmac_unsigned_public_key::Union::Curve25519(key)) => key.bytes,
                None => return Err(ContactError::BadData),
            },
            None => return Err(ContactError::BadData),
        };
        let proto_association = extract_proto_association(ik)?;

        // This will validate that the signature matches the wallet address
        let association = Eip191Association::from_proto_with_expected_address(
            key_bytes.as_slice(),
            proto_association,
            self.wallet_address.clone(),
        )?;

        Ok(association)
    }

    // The id of a contact is the base64 encoding of the keccak256 hash of the identity key
    pub fn installation_id(&self) -> String {
        key_fingerprint(&self.vmac_identity_key())
    }

    pub fn vmac_identity_key(&self) -> Curve25519PublicKey {
        let identity_key = match self.bundle.clone().version.unwrap() {
            ContactBundleVersionProto::V1(v1) => v1.identity_key.unwrap(),
        };

        let proto_key = ProtoWrapper {
            proto: identity_key,
        };

        proto_key.into()
    }

    pub fn vmac_fallback_key(&self) -> Curve25519PublicKey {
        let fallback_key = match self.bundle.clone().version.unwrap() {
            ContactBundleVersionProto::V1(v1) => v1.fallback_key.unwrap(),
        };
        let proto_key = ProtoWrapper {
            proto: fallback_key,
        };

        proto_key.into()
    }
}

impl PartialEq for Contact {
    fn eq(&self, other: &Self) -> bool {
        self.installation_id() == other.installation_id()
    }
}

impl Eq for Contact {}

impl TryFrom<Contact> for Vec<u8> {
    type Error = ContactError;

    fn try_from(contact: Contact) -> Result<Self, Self::Error> {
        (&contact).try_into()
    }
}

impl TryFrom<&Contact> for Vec<u8> {
    type Error = ContactError;

    fn try_from(contact: &Contact) -> Result<Self, Self::Error> {
        let mut buf = Vec::new();
        contact.bundle.encode(&mut buf)?;

        Ok(buf)
    }
}

fn extract_identity_key(
    bundle: InstallationContactBundle,
) -> Result<VmacAccountLinkedKey, ContactError> {
    match bundle.version {
        Some(ContactBundleVersionProto::V1(v1)) => match v1.identity_key {
            Some(key) => Ok(key),
            None => Err(ContactError::BadData),
        },
        None => Err(ContactError::BadData),
    }
}

fn extract_proto_association(
    ik: VmacAccountLinkedKey,
) -> Result<Eip191AssociationProto, ContactError> {
    let proto_association = match ik.association {
        Some(AssociationProto::Eip191(assoc)) => assoc,
        None => return Err(ContactError::BadData),
    };

    Ok(proto_association)
}

#[cfg(test)]
mod tests {
    use crate::account::{tests::test_wallet_signer, Account};

    use super::Contact;

    #[test]
    fn serialize_round_trip() {
        let account = Account::generate(test_wallet_signer).unwrap();
        let contact = account.contact();
        let contact_bytes: Vec<u8> = contact.try_into().unwrap();
        let contact2 = Contact::from_bytes(contact_bytes.clone(), account.assoc.address()).unwrap();
        let contact_2_bytes: Vec<u8> = contact2.try_into().unwrap();
        assert_eq!(contact_2_bytes, contact_bytes);
    }

    #[test]
    fn get_association() {
        let account = Account::generate(test_wallet_signer).unwrap();
        let contact = account.contact();
        let association = contact.association().unwrap();

        assert_eq!(association.address(), account.addr());
    }
}
