use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_id::associations::Identifier;

use crate::{XmtpError, foreign};

#[derive(Clone, Debug, uniffi::Enum)]
pub enum PublicIdentityKind {
    Ethereum,
    Passkey,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PublicIdentity {
    pub identifier: String,
    pub kind: PublicIdentityKind,
}

impl PublicIdentity {
    pub(crate) fn to_core(&self) -> Result<Identifier, XmtpError> {
        match self.kind {
            PublicIdentityKind::Ethereum => Identifier::eth(&self.identifier),
            PublicIdentityKind::Passkey => Identifier::passkey_str(&self.identifier, None),
        }
        .map_err(XmtpError::unknown)
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum SignerKind {
    Eoa,
    Scw {
        chain_id: u64,
        block_number: Option<u64>,
    },
    Passkey,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct SigningRequest {
    pub text: String,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum Signature {
    Ecdsa(Vec<u8>),
    Scw {
        bytes: Vec<u8>,
        address: String,
        chain_id: u64,
        block_number: Option<u64>,
    },
    Passkey {
        signature: Vec<u8>,
        public_key: Vec<u8>,
        authenticator_data: Vec<u8>,
        client_data_json: Vec<u8>,
    },
}

#[xmtp_macro::callback_error]
#[derive(Clone, Debug, thiserror::Error, uniffi::Error)]
pub enum SignerError {
    #[error("signer callback failed")]
    Failed,
}

impl From<uniffi::UnexpectedUniFFICallbackError> for SignerError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed
    }
}

// Foreign traits need `with_foreign`, which `sdk_export` cannot emit.
#[uniffi::export(with_foreign)]
#[xmtp_common::async_trait]
pub trait Signer: MaybeSend + MaybeSync + 'static {
    async fn identity(&self) -> Result<PublicIdentity, SignerError>;
    async fn kind(&self) -> Result<SignerKind, SignerError>;
    async fn sign(&self, request: SigningRequest) -> Result<Signature, SignerError>;
}

pub(crate) async fn identity(
    signer: std::sync::Arc<dyn Signer>,
) -> Result<PublicIdentity, XmtpError> {
    foreign::call(async move { signer.identity().await })
        .await
        .map_err(XmtpError::unknown)?
        .map_err(|_| XmtpError::signer())
}

pub(crate) async fn kind(signer: std::sync::Arc<dyn Signer>) -> Result<SignerKind, XmtpError> {
    foreign::call(async move { signer.kind().await })
        .await
        .map_err(XmtpError::unknown)?
        .map_err(|_| XmtpError::signer())
}

pub(crate) async fn sign(
    signer: std::sync::Arc<dyn Signer>,
    request: SigningRequest,
) -> Result<Signature, XmtpError> {
    foreign::call(async move { signer.sign(request).await })
        .await
        .map_err(XmtpError::unknown)?
        .map_err(|_| XmtpError::signer())
}
