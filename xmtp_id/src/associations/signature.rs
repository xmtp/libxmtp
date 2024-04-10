use thiserror::Error;

use super::MemberIdentifier;

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
