use prost::Message;
use xmtp_cryptography::signature::RecoverableSignature;
use xmtp_proto::xmtp::{
    message_contents::{
        signature::Union, unsigned_public_key, UnsignedPublicKey as LegacyUnsignedPublicKeyProto,
    },
    mls::message_contents::LegacyCreateIdentityAssociation as LegacyCreateIdentityAssociationProto,
};
use xmtp_v2::k256_helper;

use crate::types::Address;

use super::AssociationError;

/// An Association is link between a blockchain account and an xmtp installation for the purposes of
/// authentication.
pub(super) struct LegacyCreateIdentityAssociation {
    account_address: Address,
    installation_public_key: Vec<u8>,
    delegating_signature: Vec<u8>,
    serialized_legacy_key: Vec<u8>,
    wallet_signature: RecoverableSignature,
}

impl LegacyCreateIdentityAssociation {
    pub(crate) fn new_validated(
        installation_public_key: Vec<u8>,
        delegating_signature: Vec<u8>,
        serialized_legacy_key: Vec<u8>,
        wallet_signature: RecoverableSignature,
    ) -> Result<Self, AssociationError> {
        let account_address =
            wallet_signature.recover_address(&Self::text(&serialized_legacy_key))?;
        let this = Self {
            account_address,
            installation_public_key,
            delegating_signature,
            serialized_legacy_key,
            wallet_signature,
        };
        this.is_valid()?;
        Ok(this)
    }

    pub(crate) fn create(
        legacy_key: Vec<u8>,
        installation_public_key: Vec<u8>,
        iso8601_time: String,
    ) -> Result<Self, AssociationError> {
        todo!()
        // let account_address = owner.get_address();
        // let text = Self::text(&account_address, &installation_public_key, &iso8601_time);
        // let signature = owner.sign(&text)?;
        // Self::new_validated(
        //     account_address,
        //     installation_public_key,
        //     iso8601_time,
        //     signature,
        // )
    }

    pub(crate) fn from_proto_validated(
        proto: LegacyCreateIdentityAssociationProto,
        expected_installation_public_key: &[u8],
    ) -> Result<Self, AssociationError> {
        let delegating_signature = proto
            .signature
            .ok_or(AssociationError::MalformedAssociation)?
            .bytes;
        let legacy_signed_public_key_proto = proto
            .signed_legacy_create_identity_key
            .ok_or(AssociationError::MalformedAssociation)?;
        let serialized_legacy_key = legacy_signed_public_key_proto.key_bytes;
        let Union::WalletEcdsaCompact(wallet_ecdsa_compact) = legacy_signed_public_key_proto
            .signature
            .ok_or(AssociationError::MalformedAssociation)?
            .union
            .ok_or(AssociationError::MalformedAssociation)?
        else {
            return Err(AssociationError::MalformedAssociation);
        };
        let mut wallet_signature = wallet_ecdsa_compact.bytes.clone();
        wallet_signature.push(wallet_ecdsa_compact.recovery as u8); // TODO: normalize recovery ID if necessary
        Self::new_validated(
            expected_installation_public_key.to_vec(),
            delegating_signature,
            serialized_legacy_key,
            RecoverableSignature::Eip191Signature(wallet_signature),
        )
    }

    fn is_valid(&self) -> Result<(), AssociationError> {
        // Validate legacy key signs installation key
        let legacy_unsigned_public_key_proto =
            LegacyUnsignedPublicKeyProto::decode(self.serialized_legacy_key.as_slice())
                .or(Err(AssociationError::MalformedAssociation))?;
        let legacy_public_key_bytes = match legacy_unsigned_public_key_proto
            .union
            .ok_or(AssociationError::MalformedAssociation)?
        {
            unsigned_public_key::Union::Secp256k1Uncompressed(secp256k1_uncompressed) => {
                secp256k1_uncompressed.bytes
            }
        };
        if self.delegating_signature.len() != 65 {
            return Err(AssociationError::MalformedAssociation);
        }
        assert!(k256_helper::verify_sha256(
            &legacy_public_key_bytes,          // signed_by
            &self.installation_public_key,     // message
            &self.delegating_signature[0..64], // signature
            self.delegating_signature[64],     // recovery_id
        )
        .map_err(AssociationError::BadLegacySignature)?); // always returns true if no error

        // Validate wallet signs legacy key
        let account_address = self
            .wallet_signature
            .recover_address(&Self::text(&self.serialized_legacy_key))?;
        if self.account_address != account_address {
            Err(AssociationError::AddressMismatch {
                provided_addr: self.account_address.clone(),
                signing_addr: account_address,
            })
        } else {
            Ok(())
        }
    }

    pub fn address(&self) -> String {
        self.account_address.clone()
    }

    pub fn installation_public_key(&self) -> Vec<u8> {
        self.installation_public_key.clone()
    }

    pub fn iso8601_time(&self) -> String {
        todo!()
    }

    fn header_text() -> String {
        let label = "Create Identity".to_string();
        format!("XMTP : {}", label)
    }

    fn body_text(serialized_legacy_key: &[u8]) -> String {
        hex::encode(serialized_legacy_key)
    }

    fn footer_text() -> String {
        "For more info: https://xmtp.org/signatures/".to_string()
    }

    fn text(serialized_legacy_key: &[u8]) -> String {
        format!(
            "{}\n{}\n\n{}",
            Self::header_text(),
            Self::body_text(serialized_legacy_key),
            Self::footer_text()
        )
        .to_string()
    }
}
