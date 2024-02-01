use prost::Message;
use xmtp_proto::xmtp::{
    message_contents::{signed_private_key, SignedPrivateKey as LegacySignedPrivateKeyProto},
    mls::message_contents::LegacyCreateIdentityAssociation as LegacyCreateIdentityAssociationProto,
};
use xmtp_v2::k256_helper;

use super::{validated_legacy_signed_public_key::ValidatedLegacySignedPublicKey, AssociationError};

/// An Association is link between a blockchain account and an xmtp installation for the purposes of
/// authentication.
pub struct LegacyCreateIdentityAssociation {
    installation_public_key: Vec<u8>,
    delegating_signature: Vec<u8>,
    legacy_signed_public_key: ValidatedLegacySignedPublicKey,
}

impl LegacyCreateIdentityAssociation {
    fn new_validated(
        installation_public_key: Vec<u8>,
        delegating_signature: Vec<u8>,
        legacy_signed_public_key: ValidatedLegacySignedPublicKey,
    ) -> Result<Self, AssociationError> {
        let this = Self {
            installation_public_key,
            delegating_signature,
            legacy_signed_public_key,
        };
        this.is_valid()?;
        Ok(this)
    }

    pub(crate) fn create(
        legacy_signed_private_key: Vec<u8>,
        installation_public_key: Vec<u8>,
    ) -> Result<Self, AssociationError> {
        let legacy_signed_private_key_proto =
            LegacySignedPrivateKeyProto::decode(legacy_signed_private_key.as_slice())?;
        let signed_private_key::Union::Secp256k1(secp256k1) = legacy_signed_private_key_proto
            .union
            .ok_or(AssociationError::MalformedLegacyKey)?;
        let legacy_private_key = secp256k1.bytes;
        let (mut delegating_signature, recovery_id) = k256_helper::sign_sha256(
            &legacy_private_key,      // secret_key
            &installation_public_key, // message
        )
        .map_err(AssociationError::LegacySignature)?;
        delegating_signature.push(recovery_id); // TODO: normalize recovery ID if necessary

        let legacy_signed_public_key_proto = legacy_signed_private_key_proto
            .public_key
            .ok_or(AssociationError::MalformedLegacyKey)?;
        Self::new_validated(
            installation_public_key,
            delegating_signature,
            legacy_signed_public_key_proto.try_into()?, // ValidatedLegacySignedPublicKey
        )
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

        Self::new_validated(
            expected_installation_public_key.to_vec(),
            delegating_signature,
            legacy_signed_public_key_proto.try_into()?, // ValidatedLegacySignedPublicKey
        )
    }

    fn is_valid(&self) -> Result<(), AssociationError> {
        // Validate legacy key signs installation key
        if self.delegating_signature.len() != 65 {
            return Err(AssociationError::MalformedAssociation);
        }
        assert!(k256_helper::verify_sha256(
            &self.legacy_signed_public_key.key_bytes(), // signed_by
            &self.installation_public_key,              // message
            &self.delegating_signature[0..64],          // signature
            self.delegating_signature[64],              // recovery_id
        )
        .map_err(AssociationError::LegacySignature)?); // always returns true if no error

        // Wallet signature of legacy key is internally validated by ValidatedLegacySignedPublicKey
        Ok(())
    }

    pub fn account_address(&self) -> String {
        self.legacy_signed_public_key.account_address()
    }

    pub fn installation_public_key(&self) -> Vec<u8> {
        self.installation_public_key.clone()
    }

    pub fn created_ns(&self) -> u64 {
        self.legacy_signed_public_key.created_ns()
    }
}
