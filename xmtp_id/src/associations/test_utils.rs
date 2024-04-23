use rand::{distributions::Alphanumeric, Rng};
use xmtp_proto::{
    xmtp::identity::associations::{
        signature::Signature as SignatureKindProto, Erc1271Signature as Erc1271SignatureProto,
        LegacyDelegatedSignature as LegacyDelegatedSignatureProto,
        RecoverableEcdsaSignature as RecoverableEcdsaSignatureProto,
        RecoverableEd25519Signature as RecoverableEd25519SignatureProto,
        Signature as SignatureProto,
    },
    xmtp::message_contents::{
        Signature as LegacySignatureProto, SignedPublicKey as LegacySignedPublicKeyProto,
    },
};

use super::{MemberIdentifier, Signature, SignatureError, SignatureKind};

pub fn rand_string() -> String {
    let v: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    v
}

pub fn rand_u64() -> u64 {
    rand::thread_rng().gen()
}

pub fn rand_vec() -> Vec<u8> {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill(&mut buf[..]);
    buf.to_vec()
}

#[derive(Debug, Clone)]
pub struct MockSignature {
    is_valid: bool,
    signer_identity: MemberIdentifier,
    signature_kind: SignatureKind,
    signature_nonce: String,
}

impl MockSignature {
    pub fn new_boxed(
        is_valid: bool,
        signer_identity: MemberIdentifier,
        signature_kind: SignatureKind,
        // Signature nonce is used to control what the signature bytes are
        // Defaults to random
        signature_nonce: Option<String>,
    ) -> Box<Self> {
        let nonce = signature_nonce.unwrap_or(rand_string());
        Box::new(Self {
            is_valid,
            signer_identity,
            signature_kind,
            signature_nonce: nonce,
        })
    }
}

#[async_trait::async_trait]
impl Signature for MockSignature {
    fn signature_kind(&self) -> SignatureKind {
        self.signature_kind.clone()
    }

    async fn recover_signer(&self) -> Result<MemberIdentifier, SignatureError> {
        match self.is_valid {
            true => Ok(self.signer_identity.clone()),
            false => Err(SignatureError::Invalid),
        }
    }

    fn bytes(&self) -> Vec<u8> {
        let sig = format!("{}{}", self.signer_identity, self.signature_nonce);
        sig.as_bytes().to_vec()
    }

    fn to_proto(&self) -> SignatureProto {
        match self.signature_kind {
            SignatureKind::Erc191 => SignatureProto {
                signature: Some(SignatureKindProto::Erc191(RecoverableEcdsaSignatureProto {
                    bytes: vec![0],
                })),
            },
            SignatureKind::Erc1271 => SignatureProto {
                signature: Some(SignatureKindProto::Erc1271(Erc1271SignatureProto {
                    contract_address: "0xdead".into(),
                    block_number: 0,
                    signature: vec![0],
                })),
            },
            SignatureKind::InstallationKey => SignatureProto {
                signature: Some(SignatureKindProto::InstallationKey(
                    RecoverableEd25519SignatureProto {
                        bytes: vec![0],
                        public_key: vec![0],
                    },
                )),
            },
            SignatureKind::LegacyDelegated => SignatureProto {
                signature: Some(SignatureKindProto::DelegatedErc191(
                    LegacyDelegatedSignatureProto {
                        delegated_key: Some(LegacySignedPublicKeyProto {
                            key_bytes: vec![0],
                            signature: Some(LegacySignatureProto { union: None }),
                        }),
                        signature: Some(RecoverableEcdsaSignatureProto { bytes: vec![0] }),
                    },
                )),
            },
        }
    }
}
