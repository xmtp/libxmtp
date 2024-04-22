use std::{array::TryFromSliceError, str::FromStr};

use super::MemberIdentifier;
use ed25519_dalek::{Signature as Ed25519Signature, Verifier, VerifyingKey};
use ethers::{
    core::k256::ecdsa::VerifyingKey as EcdsaVerifyingKey,
    types::{Address, BlockNumber, U64},
    utils::hash_message,
};
use thiserror::Error;
use tokio::runtime::Runtime;
use xmtp_cryptography::signature::h160addr_to_string;
use xmtp_proto::xmtp::identity::associations::{
    signature::Signature as SignatureKindProto, Erc1271Signature as Erc1271SignatureProto,
    LegacyDelegatedSignature as LegacyDelegatedSignatureProto,
    RecoverableEcdsaSignature as RecoverableEcdsaSignatureProto,
    RecoverableEd25519Signature as RecoverableEd25519SignatureProto, Signature as SignatureProto,
};


#[derive(Debug, Error)]
pub enum SignatureError {
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
        let verifier = crate::erc1271_verifier::ERC1271Verifier::new("http://node.rpc".to_string());
        // TODO: make this function async
        let runtime = Runtime::new().unwrap();
        let is_valid = runtime.block_on(verifier.is_valid_signature(
            Address::from_slice(&self.contract_address.as_bytes()), // TODO: `from_slice` panics when input is not 20 bytes
            Some(BlockNumber::Number(U64::from(self.block_number))),
            hash_message(self.signature_text.clone()).into(), // the hash function should match the one used by the user wallet
            self.bytes().into(),
        ))?;
        if is_valid {
            Ok(MemberIdentifier::Address(self.contract_address.clone()))
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
        let signature: Ed25519Signature =
            Ed25519Signature::from_bytes(self.bytes().as_slice().try_into()?);
        let public_key: VerifyingKey =
            VerifyingKey::from_bytes(&self.public_key.as_slice().try_into()?)?;
        public_key.verify(self.signature_text.as_bytes(), &signature)?;
        Ok(MemberIdentifier::Installation(self.public_key.clone()))
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
        // 1. Verify the RecoverableEcdsaSignature and make sure it recovers to the public key specified in the `signed_public_key`
        // use ValidatedLegacySignedPublicKey
        let legacy_signer = self.legacy_key_signature.recover_signer()?;
        let signed_public_key = &EcdsaVerifyingKey::from_sec1_bytes(self.signed_public_key.key_bytes.as_slice())?;
        if legacy_signer
            != MemberIdentifier::Address(h160addr_to_string(ethers::utils::public_key_to_address(
                signed_public_key,
            )))
        {
            return Err(SignatureError::Invalid);
        }

        // 2. Verify the wallet signature on the `signed_public_key`
        // let _: ValidatedLegacySignedPublicKey = self.signed_public_key.try_into()?;
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
                    delegated_key: Some(self.signed_public_key.clone()),
                    signature: Some(RecoverableEcdsaSignatureProto {
                        bytes: self.bytes(),
                    }),
                },
            )),
        }
    }
}
