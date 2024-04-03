use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
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

pub trait Signature {
    fn recover_signer(&self) -> Result<String, SignatureError>;
    fn signature_kind(&self) -> SignatureKind;
    fn text(&self) -> String;
}
