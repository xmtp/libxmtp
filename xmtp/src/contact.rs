use prost::{DecodeError, EncodeError, Message};
use thiserror::Error;

use vodozemac::Curve25519PublicKey;
use xmtp_cryptography::hash::keccak256;
use xmtp_proto::xmtp::v3::message_contents::{
    installation_contact_bundle::Version as ContactBundleVersionProto,
    vmac_account_linked_key::Association as AssociationProto, vmac_unsigned_public_key,
    InstallationContactBundle, VmacAccountLinkedKey,
};

use crate::{
    association::{Association, AssociationError},
    utils::base64_encode,
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
    #[error("unknown error")]
    Unknown,
}
#[derive(Clone)]
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

    pub fn from_bytes(bytes: Vec<u8>, wallet_address: String) -> Result<Self, ContactError> {
        let bundle = InstallationContactBundle::decode(bytes.as_slice())?;
        let contact = Self::new(bundle, wallet_address)?;

        Ok(contact)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, ContactError> {
        let mut buf = Vec::new();
        self.bundle.encode(&mut buf)?;

        Ok(buf)
    }

    pub fn identity_key(&self) -> Result<VmacAccountLinkedKey, ContactError> {
        match self.bundle.clone().version {
            Some(ContactBundleVersionProto::V1(v1)) => match v1.identity_key {
                Some(key) => Ok(key),
                None => Err(ContactError::BadData),
            },
            None => Err(ContactError::BadData),
        }
    }

    pub fn association(&self) -> Result<Association, ContactError> {
        let ik = self.identity_key()?;
        let key_bytes = match ik.key {
            Some(key) => match key.union {
                Some(vmac_unsigned_public_key::Union::Curve25519(key)) => key.bytes,
                None => return Err(ContactError::BadData),
            },
            None => return Err(ContactError::BadData),
        };
        let proto_association = match ik.association {
            Some(AssociationProto::Eip191(assoc)) => assoc,
            None => return Err(ContactError::BadData),
        };

        // This will validate that the signature matches the wallet address
        let association = Association::from_proto(
            key_bytes.as_slice(),
            self.wallet_address.as_str(),
            proto_association,
        )?;

        Ok(association)
    }

    // The id of a contact is the base64 encoding of the keccak256 hash of the identity key
    pub fn id(&self) -> String {
        base64_encode(keccak256(self.vmac_identity_key().to_string().as_str()).as_slice())
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

#[cfg(test)]
mod tests {
    use crate::account::{tests::test_wallet_signer, Account};

    use super::Contact;

    #[test]
    fn serialize_round_trip() {
        let account = Account::generate(test_wallet_signer).unwrap();
        let contact = account.contact();
        let wallet_address = contact.wallet_address.clone();
        let contact_bytes = contact.to_bytes().unwrap();
        let contact2 = Contact::from_bytes(contact_bytes.clone(), wallet_address).unwrap();
        assert_eq!(contact2.to_bytes().unwrap(), contact_bytes);
    }

    #[test]
    fn get_association() {
        let account = Account::generate(test_wallet_signer).unwrap();
        let contact = account.contact();
        let association = contact.association().unwrap();

        assert_eq!(association.address(), account.addr());
    }
}
