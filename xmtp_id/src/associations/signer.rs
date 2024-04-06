use thiserror::Error;

use super::{MemberIdentifier, Signature, SignatureKind};

#[derive(Error, Debug)]
pub enum SignerError {
    #[error("Signature error {0}")]
    Generic(String),
}

#[async_trait::async_trait]
pub trait Signer: SignerClone {
    fn signer_identity(&self) -> MemberIdentifier;
    fn signature_kind(&self) -> SignatureKind;
    fn sign(&self, text: &str) -> Result<Box<dyn Signature>, SignerError>;
}

pub trait SignerClone {
    fn clone_box(&self) -> Box<dyn Signer>;
}

impl<T> SignerClone for T
where
    T: 'static + Signer + Clone,
{
    fn clone_box(&self) -> Box<dyn Signer> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Signer> {
    fn clone(&self) -> Box<dyn Signer> {
        self.clone_box()
    }
}
