use crate::types::Address;
use prost::Message;
use xmtp_cryptography::signature::RecoverableSignature;
use xmtp_proto::xmtp::message_contents::{
    signature::Union, unsigned_public_key, SignedPublicKey as LegacySignedPublicKeyProto,
    UnsignedPublicKey as LegacyUnsignedPublicKeyProto,
};

use super::AssociationError;

pub struct ValidatedLegacySignedPublicKey {
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
