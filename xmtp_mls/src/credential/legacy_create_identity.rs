use prost::Message;
use xmtp_cryptography::signature::RecoverableSignature;
use xmtp_proto::xmtp::{
    message_contents::{
        signature::Union, signed_private_key, unsigned_public_key,
        SignedPrivateKey as LegacySignedPrivateKeyProto,
        SignedPublicKey as LegacySignedPublicKeyProto,
        UnsignedPublicKey as LegacyUnsignedPublicKeyProto,
    },
    mls::message_contents::LegacyCreateIdentityAssociation as LegacyCreateIdentityAssociationProto,
};
use xmtp_v2::k256_helper;

use crate::types::Address;

use super::AssociationError;

struct ValidatedLegacySignedPublicKey {
    account_address: Address,
    serialized_key_data: Vec<u8>,
    wallet_signature: RecoverableSignature,
    public_key_bytes: Vec<u8>,
    created_ns: u64,
}

impl ValidatedLegacySignedPublicKey {
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

    pub fn account_address(&self) -> Address {
        self.account_address.clone()
    }

    pub fn key_bytes(&self) -> Vec<u8> {
        self.public_key_bytes.clone()
    }

    pub fn created_ns(&self) -> u64 {
        self.created_ns
    }
}

impl TryFrom<LegacySignedPublicKeyProto> for ValidatedLegacySignedPublicKey {
    type Error = AssociationError;

    fn try_from(proto: LegacySignedPublicKeyProto) -> Result<Self, AssociationError> {
        let serialized_key_data = proto.key_bytes;
        let Union::WalletEcdsaCompact(wallet_ecdsa_compact) = proto
            .signature
            .ok_or(AssociationError::MalformedLegacyKey)?
            .union
            .ok_or(AssociationError::MalformedLegacyKey)?
        else {
            return Err(AssociationError::MalformedLegacyKey);
        };
        let mut wallet_signature = wallet_ecdsa_compact.bytes.clone();
        wallet_signature.push(wallet_ecdsa_compact.recovery as u8); // TODO: normalize recovery ID if necessary
        let wallet_signature = RecoverableSignature::Eip191Signature(wallet_signature);
        let account_address =
            wallet_signature.recover_address(&Self::text(&serialized_key_data))?;
        // TODO verify this is a legitimate address

        let legacy_unsigned_public_key_proto =
            LegacyUnsignedPublicKeyProto::decode(serialized_key_data.as_slice())
                .or(Err(AssociationError::MalformedAssociation))?;
        let public_key_bytes = match legacy_unsigned_public_key_proto
            .union
            .ok_or(AssociationError::MalformedAssociation)?
        {
            unsigned_public_key::Union::Secp256k1Uncompressed(secp256k1_uncompressed) => {
                secp256k1_uncompressed.bytes
            }
        };
        let created_ns = legacy_unsigned_public_key_proto.created_ns;

        Ok(Self {
            account_address,
            wallet_signature,
            serialized_key_data,
            public_key_bytes,
            created_ns,
        })
    }
}

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
        // TODO: Sign installation key using legacy private key
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
            legacy_signed_public_key_proto.try_into()?,
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
            legacy_signed_public_key_proto.try_into()?,
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

    pub fn address(&self) -> String {
        self.legacy_signed_public_key.account_address()
    }

    pub fn installation_public_key(&self) -> Vec<u8> {
        self.installation_public_key.clone()
    }

    pub fn created_ns(&self) -> u64 {
        self.legacy_signed_public_key.created_ns()
    }
}

#[cfg(test)]
pub mod tests {
    use ethers::signers::{LocalWallet, Signer};
    use xmtp_cryptography::{signature::h160addr_to_string, utils::rng};
    use xmtp_proto::xmtp::mls::message_contents::GrantMessagingAccessAssociation as GrantMessagingAccessAssociationProto;

    use crate::credential::grant_messaging_access::GrantMessagingAccessAssociation;

    #[tokio::test]
    async fn assoc_gen() {}
}
