use crate::proto;
use crate::traits::Buffable;
use protobuf::Message;

enum SignatureType {
    ecdsa_secp256k1_sha256_compact = 1,
    wallet_personal_sign_compact = 2,
}

pub struct Signature {
    signature_type: SignatureType,
    signature_bytes: Vec<u8>,
    recovery_id: Option<u32>,
}

impl Buffable for Signature {
    fn to_proto_bytes(&self) -> Result<Vec<u8>, String> {
        // Create mutable signature proto
        let mut signature_proto = proto::signature::Signature::new();
        // Begin to construct the proto one field at a time
        match self.signature_type {
            SignatureType::ecdsa_secp256k1_sha256_compact => {
                // Create ecdsa_compact proto
                let mut ecdsa_compact_proto = proto::signature::signature::ECDSACompact::new();
                // Set signature bytes
                ecdsa_compact_proto.bytes = self.signature_bytes.to_vec();
                // Set recovery id
                ecdsa_compact_proto.recovery = self.recovery_id.unwrap().into();
                // Set ecdsa_compact proto
                signature_proto.set_ecdsa_compact(ecdsa_compact_proto);
            }
            SignatureType::wallet_personal_sign_compact => {
                // Create wallet_personal_sign_compact proto
                let mut wallet_personal_sign_compact_proto =
                    proto::signature::signature::WalletECDSACompact::new();
                // Set signature bytes
                wallet_personal_sign_compact_proto.bytes = self.signature_bytes.to_vec();
                // Set recovery id
                wallet_personal_sign_compact_proto.recovery = self.recovery_id.unwrap().into();
                // Set wallet_personal_sign_compact proto
                signature_proto.set_wallet_ecdsa_compact(wallet_personal_sign_compact_proto);
            }
        }
        signature_proto.write_to_bytes().map_err(|e| e.to_string())
    }

    fn from_proto_bytes(buff: &[u8]) -> Result<Self, String> {
        // Parse buff as Signature proto
        let signature_proto =
            proto::signature::Signature::parse_from_bytes(buff).map_err(|e| e.to_string())?;

        // Check if has_ecdsa_compact
        if signature_proto.has_ecdsa_compact() {
            let ecdsa_compact = signature_proto.ecdsa_compact();
            let signature_bytes = &ecdsa_compact.bytes;
            let recovery_id = ecdsa_compact.recovery;
            Ok(Signature {
                signature_type: SignatureType::ecdsa_secp256k1_sha256_compact,
                signature_bytes: signature_bytes.to_vec(),
                recovery_id: Some(recovery_id),
            })
        } else if signature_proto.has_wallet_ecdsa_compact() {
            let wallet_personal_sign_compact = signature_proto.wallet_ecdsa_compact();
            let signature_bytes = &wallet_personal_sign_compact.bytes;
            let recovery_id = wallet_personal_sign_compact.recovery;
            Ok(Signature {
                signature_type: SignatureType::wallet_personal_sign_compact,
                signature_bytes: signature_bytes.to_vec(),
                recovery_id: Some(recovery_id),
            })
        } else {
            Err("Signature type not supported".to_string())
        }
    }
}
