use alloy::signers::local::PrivateKeySigner;
use std::sync::Arc;
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_id::associations::Identifier;
use xmtp_id::associations::ident;
use xmtp_id::{InboxOwner, associations::unverified::UnverifiedSignature};

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

impl From<Identifier> for PublicIdentity {
    fn from(value: Identifier) -> Self {
        match value {
            Identifier::Ethereum(ident::Ethereum(identifier)) => Self {
                identifier,
                kind: PublicIdentityKind::Ethereum,
            },
            Identifier::Passkey(ident::Passkey { key, .. }) => Self {
                identifier: hex::encode(key),
                kind: PublicIdentityKind::Passkey,
            },
        }
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

struct LocalSigner(PrivateKeySigner);

#[xmtp_common::async_trait]
impl Signer for LocalSigner {
    async fn identity(&self) -> Result<PublicIdentity, SignerError> {
        Ok(PublicIdentity {
            identifier: self
                .0
                .get_identifier()
                .map_err(|_| SignerError::Failed)?
                .to_string(),
            kind: PublicIdentityKind::Ethereum,
        })
    }

    async fn kind(&self) -> Result<SignerKind, SignerError> {
        Ok(SignerKind::Eoa)
    }

    async fn sign(&self, request: SigningRequest) -> Result<Signature, SignerError> {
        let UnverifiedSignature::RecoverableEcdsa(signature) = self
            .0
            .sign(&request.text)
            .map_err(|_| SignerError::Failed)?
        else {
            return Err(SignerError::Failed);
        };
        Ok(Signature::Ecdsa(signature.signature_bytes().to_vec()))
    }
}

#[xmtp_macro::sdk_export]
pub async fn generate_local_signer() -> Arc<dyn Signer> {
    Arc::new(LocalSigner(PrivateKeySigner::random()))
}

#[xmtp_macro::sdk_export]
pub async fn local_signer_from_private_key(key: Vec<u8>) -> Result<Arc<dyn Signer>, XmtpError> {
    let signer = PrivateKeySigner::from_slice(&key)
        .map_err(|_| XmtpError::invalid("invalid local signer private key"))?;
    Ok(Arc::new(LocalSigner(signer)))
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
