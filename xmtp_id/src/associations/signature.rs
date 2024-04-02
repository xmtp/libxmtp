use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum SignatureError {
    #[error("Signature validation failed")]
    Invalid,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SignatureKind {
    Erc191,
    Erc1271,
    InstallationKey,
    LegacyKey,
}

pub trait Signature {
    fn recover_signer(&self) -> Result<String, SignatureError>;
    fn signature_kind(&self) -> SignatureKind;
    fn text(&self) -> String;
}
