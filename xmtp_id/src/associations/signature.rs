use thiserror::Error;

use super::MemberIdentifier;
use xmtp_proto::xmtp::identity::associations::{
    signature::Signature as SignatureKindProto, Erc1271Signature as Erc1271SignatureProto,
    LegacyDelegatedSignature as LegacyDelegatedSignatureProto,
    RecoverableEcdsaSignature as RecoverableEcdsaSignatureProto,
    RecoverableEd25519Signature as RecoverableEd25519SignatureProto, Signature as SignatureProto,
};

#[derive(Debug, Error)]
pub enum SignatureError {
    #[error("Signature validation failed")]
    Invalid,
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

pub trait Signature: SignatureClone {
    fn recover_signer(&self) -> Result<MemberIdentifier, SignatureError>;
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
#[derive(Clone)]
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

impl Signature for RecoverableEcdsaSignature {
    fn recover_signer(&self) -> Result<MemberIdentifier, SignatureError> {
        todo!()
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

#[allow(dead_code)]
#[derive(Clone)]
pub struct Erc1271Signature {
    signature_text: String,
    signature_bytes: Vec<u8>,
    contract_address: String,
    block_number: u64,
}

impl Erc1271Signature {
    pub fn new(
        signature_text: String,
        signature_bytes: Vec<u8>,
        contract_address: String,
        block_number: u64,
    ) -> Self {
        Erc1271Signature {
            signature_text,
            signature_bytes,
            contract_address,
            block_number,
        }
    }
}

impl Signature for Erc1271Signature {
    fn recover_signer(&self) -> Result<MemberIdentifier, SignatureError> {
        // TODO: Verify signature first
        Ok(self.contract_address.clone().into())
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
                contract_address: self.contract_address.clone(),
                block_number: self.block_number,
                signature: self.bytes(),
            })),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct InstallationKeySignature {
    signature_text: String,
    signature_bytes: Vec<u8>,
    public_key: Vec<u8>,
}

impl InstallationKeySignature {
    pub fn new(signature_text: String, signature_bytes: Vec<u8>, public_key: Vec<u8>) -> Self {
        InstallationKeySignature {
            signature_text,
            signature_bytes,
            public_key,
        }
    }
}

impl Signature for InstallationKeySignature {
    fn recover_signer(&self) -> Result<MemberIdentifier, SignatureError> {
        todo!()
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
                    public_key: self.public_key.clone(),
                },
            )),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct LegacyDelegatedSignature {
    // This would be the signature from the legacy key
    legacy_key_signature: RecoverableEcdsaSignature,
    signed_public_key: xmtp_proto::xmtp::message_contents::SignedPublicKey,
}

impl LegacyDelegatedSignature {
    pub fn new(
        legacy_key_signature: RecoverableEcdsaSignature,
        signed_public_key: xmtp_proto::xmtp::message_contents::SignedPublicKey,
    ) -> Self {
        LegacyDelegatedSignature {
            legacy_key_signature,
            signed_public_key,
        }
    }
}

impl Signature for LegacyDelegatedSignature {
    fn recover_signer(&self) -> Result<MemberIdentifier, SignatureError> {
        // TODO: Two steps needed here:
        // 1. Verify the RecoverableEcdsaSignature and make sure it recovers to the public key specified in the `signed_public_key`
        // 2. Verify the wallet signature on the `signed_public_key`
        // Return the wallet address
        todo!()
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
                    delegated_key: Some(self.signed_public_key.clone()),
                    signature: Some(RecoverableEcdsaSignatureProto {
                        bytes: self.bytes(),
                    }),
                },
            )),
        }
    }
}
