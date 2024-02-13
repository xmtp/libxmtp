use log::info;
use prost::Message;

use xmtp_cryptography::signature::RecoverableSignature;
use xmtp_proto::xmtp::message_contents::{
    signature::{Union, WalletEcdsaCompact},
    unsigned_public_key, Signature as SignatureProto,
    SignedPublicKey as LegacySignedPublicKeyProto,
    UnsignedPublicKey as LegacyUnsignedPublicKeyProto,
};

use super::AssociationError;
use crate::{types::Address, utils::address::sanitize_evm_addresses};

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
        let union = proto
            .signature
            .ok_or(AssociationError::MalformedLegacyKey(
                "Missing signature field".to_string(),
            ))?
            .union
            .ok_or(AssociationError::MalformedLegacyKey(
                "Missing signature.union field".to_string(),
            ))?;
        let wallet_signature = match union {
            Union::WalletEcdsaCompact(wallet_ecdsa_compact) => {
                info!("Reading WalletEcdsaCompact from legacy key");
                let mut wallet_signature = wallet_ecdsa_compact.bytes.clone();
                wallet_signature.push(wallet_ecdsa_compact.recovery as u8); // TODO: normalize recovery ID if necessary
                if wallet_signature.len() != 65 {
                    return Err(AssociationError::MalformedAssociation);
                }
                wallet_signature
            }
            Union::EcdsaCompact(ecdsa_compact) => {
                info!("Reading EcdsaCompact from legacy key");
                let mut signature = ecdsa_compact.bytes.clone();
                signature.push(ecdsa_compact.recovery as u8); // TODO: normalize recovery ID if necessary
                if signature.len() != 65 {
                    return Err(AssociationError::MalformedAssociation);
                }
                signature
            }
        };
        let wallet_signature = RecoverableSignature::Eip191Signature(wallet_signature);
        let account_address =
            wallet_signature.recover_address(&Self::text(&serialized_key_data))?;
        let account_address = sanitize_evm_addresses(vec![account_address])?[0].clone();

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
            wallet_signature: wallet_signature,
            serialized_key_data: serialized_key_data,
            public_key_bytes,
            created_ns,
        })
    }
}

impl From<ValidatedLegacySignedPublicKey> for LegacySignedPublicKeyProto {
    fn from(validated: ValidatedLegacySignedPublicKey) -> Self {
        let RecoverableSignature::Eip191Signature(signature) = validated.wallet_signature;
        Self {
            key_bytes: validated.serialized_key_data,
            signature: Some(SignatureProto {
                union: Some(Union::WalletEcdsaCompact(WalletEcdsaCompact {
                    bytes: signature[0..64].to_vec(),
                    recovery: signature[64] as u32,
                })),
            }),
        }
    }
}

#[cfg(test)]
pub mod tests {
    use crate::{assert_err, credential::AssociationError};

    use super::ValidatedLegacySignedPublicKey;
    use prost::Message;
    use xmtp_proto::xmtp::message_contents::SignedPublicKey as LegacySignedPublicKeyProto;

    #[tokio::test]
    async fn validate_good_key_round_trip() {
        let proto_bytes = vec![
            10, 79, 8, 192, 195, 165, 174, 203, 153, 231, 213, 23, 26, 67, 10, 65, 4, 216, 84, 174,
            252, 198, 225, 219, 168, 239, 166, 62, 233, 206, 108, 53, 155, 87, 132, 8, 43, 91, 36,
            91, 81, 93, 213, 67, 241, 69, 5, 31, 249, 186, 129, 119, 144, 4, 44, 54, 76, 185, 95,
            61, 23, 231, 72, 7, 169, 18, 70, 113, 79, 173, 82, 13, 37, 146, 201, 43, 174, 180, 33,
            125, 43, 18, 70, 18, 68, 10, 64, 7, 136, 100, 172, 155, 247, 230, 255, 253, 247, 78,
            50, 212, 226, 41, 78, 239, 183, 136, 247, 122, 88, 155, 245, 219, 183, 215, 202, 42,
            89, 162, 128, 96, 96, 120, 131, 17, 70, 38, 231, 2, 27, 91, 29, 66, 110, 128, 140, 1,
            42, 217, 185, 2, 181, 208, 100, 143, 143, 219, 159, 174, 1, 233, 191, 16, 1,
        ];
        let account_address = "0x220ca99fb7fafa18cb623d924794dde47b4bc2e9";

        let proto = LegacySignedPublicKeyProto::decode(proto_bytes.as_slice()).unwrap();
        let validated_key = ValidatedLegacySignedPublicKey::try_from(proto)
            .expect("Key should validate successfully");
        let proto: LegacySignedPublicKeyProto = validated_key.into();
        let validated_key = ValidatedLegacySignedPublicKey::try_from(proto)
            .expect("Key should still validate successfully");
        assert_eq!(validated_key.account_address(), account_address);
    }

    #[tokio::test]
    async fn validate_malformed_key() {
        let proto_bytes = vec![
            10, 79, 8, 192, 195, 165, 174, 203, 153, 231, 213, 23, 26, 67, 10, 65, 4, 216, 84, 174,
            252, 198, 225, 219, 168, 239, 166, 62, 233, 206, 108, 53, 155, 87, 132, 8, 43, 91, 36,
            91, 81, 93, 213, 67, 241, 69, 5, 31, 249, 186, 129, 119, 144, 4, 44, 54, 76, 185, 95,
            61, 23, 231, 72, 7, 169, 18, 70, 113, 79, 173, 82, 13, 37, 146, 201, 43, 174, 180, 33,
            125, 43, 18, 70, 18, 68, 10, 64, 7, 136, 100, 172, 155, 247, 230, 255, 253, 247, 78,
            50, 212, 226, 41, 78, 239, 183, 136, 247, 122, 88, 155, 245, 219, 183, 215, 202, 42,
            89, 162, 128, 96, 96, 120, 131, 17, 70, 38, 231, 2, 27, 91, 29, 66, 110, 128, 140, 1,
            42, 217, 185, 2, 181, 208, 100, 143, 143, 219, 159, 174, 1, 233, 191, 16, 1,
        ];
        let mut proto = LegacySignedPublicKeyProto::decode(proto_bytes.as_slice()).unwrap();
        proto.key_bytes[0] += 1; // Corrupt the serialized key data
        assert_err!(
            ValidatedLegacySignedPublicKey::try_from(proto),
            AssociationError::MalformedAssociation
        );
    }
}
