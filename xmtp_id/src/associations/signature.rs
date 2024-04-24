use std::array::TryFromSliceError;

use crate::constants::INSTALLATION_KEY_SIGNATURE_CONTEXT;

use super::MemberIdentifier;
use async_trait::async_trait;
use ed25519_dalek::{Signature as Ed25519Signature, VerifyingKey};
use ethers::{
    types::{BlockNumber, U64},
    utils::hash_message,
};
use sha2::{Digest, Sha512};
use thiserror::Error;
use xmtp_cryptography::signature::{h160addr_to_string, sanitize_evm_addresses};
use xmtp_proto::xmtp::identity::associations::{
    signature::Signature as SignatureKindProto, Erc1271Signature as Erc1271SignatureProto,
    LegacyDelegatedSignature as LegacyDelegatedSignatureProto,
    RecoverableEcdsaSignature as RecoverableEcdsaSignatureProto,
    RecoverableEd25519Signature as RecoverableEd25519SignatureProto, Signature as SignatureProto,
};

#[derive(Debug, Error)]
pub enum SignatureError {
    #[error(transparent)]
    CryptoSignatureError(#[from] xmtp_cryptography::signature::SignatureError),
    #[error(transparent)]
    ECDSAError(#[from] ethers::types::SignatureError),
    #[error(transparent)]
    VerifierError(#[from] crate::erc1271_verifier::VerifierError),
    #[error(transparent)]
    Ed25519Error(#[from] ed25519_dalek::SignatureError),
    #[error(transparent)]
    TryFromSliceError(#[from] TryFromSliceError),
    #[error("Signature validation failed")]
    Invalid,
    #[error(transparent)]
    AddressValidationError(#[from] xmtp_cryptography::signature::AddressValidationError),
    #[error("Invalid account address")]
    InvalidAccountAddress(#[from] rustc_hex::FromHexError),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SignatureKind {
    // We might want to have some sort of LegacyErc191 Signature Kind for the `CreateIdentity` signatures only
    Erc191,
    Erc1271,
    InstallationKey,
    LegacyDelegated,
}

impl std::fmt::Display for SignatureKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SignatureKind::Erc191 => write!(f, "erc-191"),
            SignatureKind::Erc1271 => write!(f, "erc-1271"),
            SignatureKind::InstallationKey => write!(f, "installation-key"),
            SignatureKind::LegacyDelegated => write!(f, "legacy-delegated"),
        }
    }
}

#[async_trait]
pub trait Signature: SignatureClone + std::fmt::Debug + Send + Sync + 'static {
    async fn recover_signer(&self) -> Result<MemberIdentifier, SignatureError>;
    fn signature_kind(&self) -> SignatureKind;
    fn bytes(&self) -> Vec<u8>;
    fn to_proto(&self) -> SignatureProto;
}

pub trait SignatureClone {
    fn clone_box(&self) -> Box<dyn Signature>;
}

impl<T> SignatureClone for T
where
    T: 'static + Signature + Clone,
{
    fn clone_box(&self) -> Box<dyn Signature> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Signature> {
    fn clone(&self) -> Box<dyn Signature> {
        self.clone_box()
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RecoverableEcdsaSignature {
    signature_text: String,
    signature_bytes: Vec<u8>,
}

impl RecoverableEcdsaSignature {
    pub fn new(signature_text: String, signature_bytes: Vec<u8>) -> Self {
        RecoverableEcdsaSignature {
            signature_text,
            signature_bytes,
        }
    }
}

#[async_trait]
impl Signature for RecoverableEcdsaSignature {
    async fn recover_signer(&self) -> Result<MemberIdentifier, SignatureError> {
        let signature = ethers::types::Signature::try_from(self.bytes().as_slice())?;
        Ok(MemberIdentifier::Address(h160addr_to_string(
            signature.recover(self.signature_text.clone())?,
        )))
    }

    fn signature_kind(&self) -> SignatureKind {
        SignatureKind::Erc191
    }

    fn bytes(&self) -> Vec<u8> {
        self.signature_bytes.clone()
    }

    fn to_proto(&self) -> SignatureProto {
        SignatureProto {
            signature: Some(SignatureKindProto::Erc191(RecoverableEcdsaSignatureProto {
                bytes: self.bytes(),
            })),
        }
    }
}

// CAIP-10[https://github.com/ChainAgnostic/CAIPs/blob/main/CAIPs/caip-10.md]
#[derive(Debug, Clone)]
pub struct AccountId {
    pub(crate) chain_id: String,
    pub(crate) account_address: String,
}

impl AccountId {
    pub fn is_evm_chain(&self) -> bool {
        self.chain_id.starts_with("eip155")
    }
    pub fn get_account_address(&self) -> &str {
        &self.account_address
    }
}

#[derive(Debug, Clone)]
pub struct Erc1271Signature {
    signature_text: String,
    signature_bytes: Vec<u8>,
    account_id: AccountId,
    block_number: u64,
    chain_rpc_url: String,
}

impl Erc1271Signature {
    pub fn new(
        signature_text: String,
        signature_bytes: Vec<u8>,
        account_id: AccountId,
        chain_rpc_url: String,
        block_number: u64,
    ) -> Self {
        Erc1271Signature {
            signature_text,
            signature_bytes,
            account_id,
            chain_rpc_url,
            block_number,
        }
    }
}

#[async_trait]
impl Signature for Erc1271Signature {
    async fn recover_signer(&self) -> Result<MemberIdentifier, SignatureError> {
        let verifier = crate::erc1271_verifier::ERC1271Verifier::new(self.chain_rpc_url.clone());
        let is_valid = verifier
            .is_valid_signature(
                self.account_id.get_account_address().parse()?,
                Some(BlockNumber::Number(U64::from(self.block_number))),
                hash_message(self.signature_text.clone()).into(), // the hash function should match the one used by the user wallet
                self.bytes().into(),
            )
            .await?;
        if is_valid {
            Ok(MemberIdentifier::Address(
                self.account_id.get_account_address().to_string(),
            ))
        } else {
            Err(SignatureError::Invalid)
        }
    }

    fn signature_kind(&self) -> SignatureKind {
        SignatureKind::Erc1271
    }

    fn bytes(&self) -> Vec<u8> {
        self.signature_bytes.clone()
    }

    fn to_proto(&self) -> SignatureProto {
        SignatureProto {
            signature: Some(SignatureKindProto::Erc1271(Erc1271SignatureProto {
                account_id: self.account_id.clone().into(),
                block_number: self.block_number,
                signature: self.bytes(),
            })),
        }
    }
}

#[derive(Clone, Debug)]
pub struct InstallationKeySignature {
    signature_text: String,
    signature_bytes: Vec<u8>,
    verifying_key: Vec<u8>,
}

impl InstallationKeySignature {
    pub fn new(signature_text: String, signature_bytes: Vec<u8>, verifying_key: Vec<u8>) -> Self {
        InstallationKeySignature {
            signature_text,
            signature_bytes,
            verifying_key,
        }
    }
}

#[async_trait]
impl Signature for InstallationKeySignature {
    async fn recover_signer(&self) -> Result<MemberIdentifier, SignatureError> {
        let signature: Ed25519Signature =
            Ed25519Signature::from_bytes(self.bytes().as_slice().try_into()?);
        let verifying_key: VerifyingKey =
            VerifyingKey::from_bytes(&self.verifying_key.as_slice().try_into()?)?;
        let mut prehashed: Sha512 = Sha512::new();
        prehashed.update(self.signature_text.clone());
        verifying_key.verify_prehashed(
            prehashed,
            Some(INSTALLATION_KEY_SIGNATURE_CONTEXT),
            &signature,
        )?;
        Ok(MemberIdentifier::Installation(self.verifying_key.clone()))
    }

    fn signature_kind(&self) -> SignatureKind {
        SignatureKind::InstallationKey
    }

    fn bytes(&self) -> Vec<u8> {
        self.signature_bytes.clone()
    }

    fn to_proto(&self) -> SignatureProto {
        SignatureProto {
            signature: Some(SignatureKindProto::InstallationKey(
                RecoverableEd25519SignatureProto {
                    bytes: self.bytes(),
                    public_key: self.verifying_key.clone(),
                },
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LegacyDelegatedSignature {
    // This would be the signature from the legacy key
    legacy_key_signature: RecoverableEcdsaSignature,
    signed_public_key: ValidatedLegacySignedPublicKey,
}

impl LegacyDelegatedSignature {
    pub fn new(
        legacy_key_signature: RecoverableEcdsaSignature,
        signed_public_key: ValidatedLegacySignedPublicKey,
    ) -> Self {
        LegacyDelegatedSignature {
            legacy_key_signature,
            signed_public_key,
        }
    }
}

#[async_trait]
impl Signature for LegacyDelegatedSignature {
    async fn recover_signer(&self) -> Result<MemberIdentifier, SignatureError> {
        // 1. Verify the RecoverableEcdsaSignature
        let legacy_signer = self.legacy_key_signature.recover_signer().await?;

        // 2. Signed public key is already verified, we just make sure it matches to the legacy_signer
        if MemberIdentifier::Address(self.signed_public_key.account_address()) != legacy_signer {
            return Err(SignatureError::Invalid);
        }

        Ok(legacy_signer)
    }

    fn signature_kind(&self) -> SignatureKind {
        SignatureKind::LegacyDelegated
    }

    fn bytes(&self) -> Vec<u8> {
        self.legacy_key_signature.bytes()
    }

    fn to_proto(&self) -> SignatureProto {
        SignatureProto {
            signature: Some(SignatureKindProto::DelegatedErc191(
                LegacyDelegatedSignatureProto {
                    delegated_key: Some(self.signed_public_key.clone().into()),
                    signature: Some(RecoverableEcdsaSignatureProto {
                        bytes: self.bytes(),
                    }),
                },
            )),
        }
    }
}

use prost::Message;

use xmtp_cryptography::signature::RecoverableSignature;
use xmtp_proto::xmtp::message_contents::{
    signature::{Union, WalletEcdsaCompact},
    unsigned_public_key, Signature as SignedPublicKeySignatureProto,
    SignedPublicKey as LegacySignedPublicKeyProto,
    UnsignedPublicKey as LegacyUnsignedPublicKeyProto,
};

#[derive(Clone, Debug)]
pub struct ValidatedLegacySignedPublicKey {
    account_address: String,
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

    pub fn account_address(&self) -> String {
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
    type Error = SignatureError;

    fn try_from(proto: LegacySignedPublicKeyProto) -> Result<Self, Self::Error> {
        let serialized_key_data = proto.key_bytes;
        let union = proto
            .signature
            .ok_or(SignatureError::Invalid)?
            .union
            .ok_or(SignatureError::Invalid)?;
        let wallet_signature = match union {
            Union::WalletEcdsaCompact(wallet_ecdsa_compact) => {
                let mut wallet_signature = wallet_ecdsa_compact.bytes.clone();
                wallet_signature.push(wallet_ecdsa_compact.recovery as u8); // TODO: normalize recovery ID if necessary
                if wallet_signature.len() != 65 {
                    return Err(SignatureError::Invalid);
                }
                wallet_signature
            }
            Union::EcdsaCompact(ecdsa_compact) => {
                let mut signature = ecdsa_compact.bytes.clone();
                signature.push(ecdsa_compact.recovery as u8); // TODO: normalize recovery ID if necessary
                if signature.len() != 65 {
                    return Err(SignatureError::Invalid);
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
                .or(Err(SignatureError::Invalid))?;
        let public_key_bytes = match legacy_unsigned_public_key_proto
            .union
            .ok_or(SignatureError::Invalid)?
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

impl From<ValidatedLegacySignedPublicKey> for LegacySignedPublicKeyProto {
    fn from(validated: ValidatedLegacySignedPublicKey) -> Self {
        let RecoverableSignature::Eip191Signature(signature) = validated.wallet_signature;
        Self {
            key_bytes: validated.serialized_key_data,
            signature: Some(SignedPublicKeySignatureProto {
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

    use super::*;
    use crate::{
        associations::{
            signature::Signature,
            test_utils::{rand_u64, MockSignature},
            unsigned_actions::{SignatureTextCreator, UnsignedAddAssociation, UnsignedCreateInbox},
        },
        InboxOwner,
    };
    use ed25519_dalek::SigningKey;
    use ethers::prelude::*;
    use prost::Message;
    use sha2::Digest;
    use sha2::Sha512;
    use xmtp_proto::xmtp::message_contents::SignedPublicKey as LegacySignedPublicKeyProto;

    #[test]
    fn validate_good_key_round_trip() {
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

    #[test]
    fn validate_malformed_key() {
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
        assert!(matches!(
            ValidatedLegacySignedPublicKey::try_from(proto),
            Err(super::SignatureError::Invalid)
        ));
    }

    #[tokio::test]
    async fn recover_signer_ecdsa() {
        let wallet: LocalWallet = LocalWallet::new(&mut rand::thread_rng());
        let unsigned_action = UnsignedCreateInbox {
            nonce: rand_u64(),
            account_address: wallet.get_address(),
        };
        let signature_text = unsigned_action.signature_text();
        let signature_bytes: Vec<u8> = wallet
            .sign_message(signature_text.clone())
            .await
            .unwrap()
            .to_vec();
        let signature = RecoverableEcdsaSignature::new(signature_text.clone(), signature_bytes);
        let expected = MemberIdentifier::Address(wallet.get_address());
        let actual = signature.recover_signer().await.unwrap();

        assert_eq!(expected, actual);
    }

    #[tokio::test]
    #[ignore]
    async fn recover_signer_erc1271() {
        let wallet: LocalWallet = LocalWallet::new(&mut rand::thread_rng());

        let mock_erc1271 = MockSignature::new_boxed(
            true,
            MemberIdentifier::Address(wallet.get_address()),
            SignatureKind::Erc1271,
            None,
        );

        let expected = MemberIdentifier::Address(wallet.get_address());
        let actual = mock_erc1271.recover_signer().await.unwrap();
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn recover_signer_installation() {
        let signing_key: SigningKey = SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();

        let unsigned_action = UnsignedAddAssociation {
            new_member_identifier: MemberIdentifier::Address("0x123456789abcdef".to_string()),
        };
        let signature_text = unsigned_action.signature_text();
        let mut prehashed: Sha512 = Sha512::new();
        prehashed.update(signature_text.clone());
        let sig = signing_key
            .sign_prehashed(prehashed, Some(INSTALLATION_KEY_SIGNATURE_CONTEXT))
            .unwrap();
        let installation_key_sig = InstallationKeySignature::new(
            signature_text.clone(),
            sig.to_vec(),
            verifying_key.as_bytes().to_vec(),
        );
        let expected = MemberIdentifier::Installation(verifying_key.as_bytes().to_vec());
        let actual = installation_key_sig.recover_signer().await.unwrap();
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn recover_signer_legacy() {
        // 1. RecoverableEcdsaSignature
        let legacy_key: LocalWallet = LocalWallet::new(&mut rand::thread_rng());
        let unsigned_action = UnsignedCreateInbox {
            nonce: rand_u64(),
            account_address: legacy_key.get_address(),
        };
        let signature_text = unsigned_action.signature_text();
        let signature_bytes: Vec<u8> = legacy_key
            .sign_message(signature_text.clone())
            .await
            .unwrap()
            .to_vec();
        let signature = RecoverableEcdsaSignature::new(signature_text.clone(), signature_bytes);

        // 2. ValidatedLegacySignedPublicKey
        let signed_public_key = ValidatedLegacySignedPublicKey {
            account_address: legacy_key.get_address(),
            serialized_key_data: vec![],
            wallet_signature: RecoverableSignature::Eip191Signature(vec![0; 65]),
            public_key_bytes: vec![0; 32],
            created_ns: 0,
        };

        // LegacyDelegatedSignature
        let delegated_signature = LegacyDelegatedSignature::new(signature, signed_public_key);
        let expected = MemberIdentifier::Address(legacy_key.get_address());
        let actual = delegated_signature.recover_signer().await.unwrap();
        assert_eq!(expected, actual);
    }
}
