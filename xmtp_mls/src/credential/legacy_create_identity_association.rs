use prost::Message;

use xmtp_id::associations::signature::ValidatedLegacySignedPublicKey;
use xmtp_proto::xmtp::{
    message_contents::{signed_private_key, SignedPrivateKey as LegacySignedPrivateKeyProto},
    mls::message_contents::{
        LegacyCreateIdentityAssociation as LegacyCreateIdentityAssociationProto,
        RecoverableEcdsaSignature as RecoverableEcdsaSignatureProto,
    },
};
use xmtp_v2::k256_helper;

use super::AssociationError;

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
            .ok_or(AssociationError::MalformedLegacyKey(
                "Missing secp256k1.union field".to_string(),
            ))?;
        let legacy_private_key = secp256k1.bytes;
        let (mut delegating_signature, recovery_id) = k256_helper::sign_sha256(
            &legacy_private_key,      // secret_key
            &installation_public_key, // message
        )
        .map_err(AssociationError::LegacySignature)?;
        delegating_signature.push(recovery_id); // TODO: normalize recovery ID if necessary

        let legacy_signed_public_key_proto = legacy_signed_private_key_proto.public_key.ok_or(
            AssociationError::MalformedLegacyKey("Missing public_key field".to_string()),
        )?;
        Self::new_validated(
            installation_public_key,
            delegating_signature,
            legacy_signed_public_key_proto.try_into()?, // ValidatedLegacySignedPublicKey
        )
    }

    pub fn from_proto_validated(
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

        // Wallet signature of legacy key is internally validated by ValidatedLegacySignedPublicKey on creation
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

impl From<LegacyCreateIdentityAssociation> for LegacyCreateIdentityAssociationProto {
    fn from(assoc: LegacyCreateIdentityAssociation) -> Self {
        Self {
            signature: Some(RecoverableEcdsaSignatureProto {
                bytes: assoc.delegating_signature.clone(),
            }),
            signed_legacy_create_identity_key: Some(assoc.legacy_signed_public_key.into()),
        }
    }
}

#[cfg(test)]
pub mod tests {
    use openmls_basic_credential::SignatureKeyPair;
    use xmtp_proto::xmtp::mls::message_contents::LegacyCreateIdentityAssociation as LegacyCreateIdentityAssociationProto;

    use crate::{
        assert_err,
        configuration::CIPHERSUITE,
        credential::{
            legacy_create_identity_association::LegacyCreateIdentityAssociation, AssociationError,
        },
    };

    #[tokio::test]
    async fn validate_serialization_round_trip() {
        let legacy_address = "0x419cb1fa5635b0c6df47c9dc5765c8f1f4dff78e";
        let legacy_signed_private_key_proto = vec![
            8, 128, 154, 196, 133, 220, 244, 197, 216, 23, 18, 34, 10, 32, 214, 70, 104, 202, 68,
            204, 25, 202, 197, 141, 239, 159, 145, 249, 55, 242, 147, 126, 3, 124, 159, 207, 96,
            135, 134, 122, 60, 90, 82, 171, 131, 162, 26, 153, 1, 10, 79, 8, 128, 154, 196, 133,
            220, 244, 197, 216, 23, 26, 67, 10, 65, 4, 232, 32, 50, 73, 113, 99, 115, 168, 104,
            229, 206, 24, 217, 132, 223, 217, 91, 63, 137, 136, 50, 89, 82, 186, 179, 150, 7, 127,
            140, 10, 165, 117, 233, 117, 196, 134, 227, 143, 125, 210, 187, 77, 195, 169, 162, 116,
            34, 20, 196, 145, 40, 164, 246, 139, 197, 154, 233, 190, 148, 35, 131, 240, 106, 103,
            18, 70, 18, 68, 10, 64, 90, 24, 36, 99, 130, 246, 134, 57, 60, 34, 142, 165, 221, 123,
            63, 27, 138, 242, 195, 175, 212, 146, 181, 152, 89, 48, 8, 70, 104, 94, 163, 0, 25,
            196, 228, 190, 49, 108, 141, 60, 174, 150, 177, 115, 229, 138, 92, 105, 170, 226, 204,
            249, 206, 12, 37, 145, 3, 35, 226, 15, 49, 20, 102, 60, 16, 1,
        ];
        let installation_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();

        let assoc = LegacyCreateIdentityAssociation::create(
            legacy_signed_private_key_proto,
            installation_keys.to_public_vec(),
        )
        .unwrap();

        let proto: LegacyCreateIdentityAssociationProto = assoc.into();
        let assoc = LegacyCreateIdentityAssociation::from_proto_validated(
            proto,
            &installation_keys.to_public_vec(),
        )
        .unwrap();
        assert_eq!(assoc.account_address(), legacy_address);
        assert_eq!(
            assoc.installation_public_key(),
            installation_keys.to_public_vec()
        );
    }

    #[tokio::test]
    async fn validate_bad_signature() {
        // let legacy_address = "0x419Cb1fA5635b0c6Df47c9DC5765c8f1f4DfF78e";
        let legacy_signed_private_key_proto = vec![
            8, 128, 154, 196, 133, 220, 244, 197, 216, 23, 18, 34, 10, 32, 214, 70, 104, 202, 68,
            204, 25, 202, 197, 141, 239, 159, 145, 249, 55, 242, 147, 126, 3, 124, 159, 207, 96,
            135, 134, 122, 60, 90, 82, 171, 131, 162, 26, 153, 1, 10, 79, 8, 128, 154, 196, 133,
            220, 244, 197, 216, 23, 26, 67, 10, 65, 4, 232, 32, 50, 73, 113, 99, 115, 168, 104,
            229, 206, 24, 217, 132, 223, 217, 91, 63, 137, 136, 50, 89, 82, 186, 179, 150, 7, 127,
            140, 10, 165, 117, 233, 117, 196, 134, 227, 143, 125, 210, 187, 77, 195, 169, 162, 116,
            34, 20, 196, 145, 40, 164, 246, 139, 197, 154, 233, 190, 148, 35, 131, 240, 106, 103,
            18, 70, 18, 68, 10, 64, 90, 24, 36, 99, 130, 246, 134, 57, 60, 34, 142, 165, 221, 123,
            63, 27, 138, 242, 195, 175, 212, 146, 181, 152, 89, 48, 8, 70, 104, 94, 163, 0, 25,
            196, 228, 190, 49, 108, 141, 60, 174, 150, 177, 115, 229, 138, 92, 105, 170, 226, 204,
            249, 206, 12, 37, 145, 3, 35, 226, 15, 49, 20, 102, 60, 16, 1,
        ];
        let installation_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();

        let mut assoc = LegacyCreateIdentityAssociation::create(
            legacy_signed_private_key_proto,
            installation_keys.to_public_vec(),
        )
        .unwrap();
        assoc.delegating_signature[0] ^= 1;

        let proto: LegacyCreateIdentityAssociationProto = assoc.into();
        assert_err!(
            LegacyCreateIdentityAssociation::from_proto_validated(
                proto,
                &installation_keys.to_public_vec(),
            ),
            AssociationError::LegacySignature(_)
        );
    }
}
