#![warn(clippy::unwrap_used)]

pub mod associations;
pub mod constants;
pub mod scw_verifier;
pub mod utils;

pub use alloy::primitives::{BlockNumber, Bytes};
use alloy::{signers::SignerSync, signers::local::PrivateKeySigner};
use associations::{
    Identifier,
    unverified::{UnverifiedRecoverableEcdsaSignature, UnverifiedSignature},
};
use openmls_traits::types::CryptoError;
use thiserror::Error;
use xmtp_cryptography::signature::{IdentifierValidationError, SignatureError, h160addr_to_string};

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("generating key-pairs: {0}")]
    KeyGenerationError(#[from] CryptoError),
    #[error("uninitialized identity")]
    UninitializedIdentity,
    #[error("protobuf deserialization: {0}")]
    Deserialization(#[from] prost::DecodeError),
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),
    #[error("MLS signer error {0}")]
    Signing(#[from] xmtp_cryptography::SignerError),
}

/// The global InboxID Reference Type.
pub type InboxIdRef<'a> = &'a str;

/// Global InboxID Owned Type.
pub type InboxId = String;

pub type WalletAddress = String;

use crate::associations::unverified::UnverifiedIdentityUpdate;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_response::IdentityUpdateLog;

#[derive(Clone)]
pub struct InboxUpdate {
    pub sequence_id: u64,
    pub server_timestamp_ns: u64,
    pub update: UnverifiedIdentityUpdate,
}

impl TryFrom<IdentityUpdateLog> for InboxUpdate {
    type Error = ConversionError;

    fn try_from(update: IdentityUpdateLog) -> Result<Self, Self::Error> {
        Ok(Self {
            sequence_id: update.sequence_id,
            server_timestamp_ns: update.server_timestamp_ns,
            update: update
                .update
                .ok_or(ConversionError::Missing {
                    item: "update",
                    r#type: std::any::type_name::<IdentityUpdateLog>(),
                })?
                .try_into()?,
        })
    }
}

pub trait AsIdRef: Send + Sync {
    fn as_ref(&'_ self) -> InboxIdRef<'_>;
}

impl AsIdRef for InboxId {
    fn as_ref(&self) -> InboxIdRef<'_> {
        self
    }
}
impl AsIdRef for &InboxId {
    fn as_ref(&self) -> InboxIdRef<'_> {
        self
    }
}
impl AsIdRef for InboxIdRef<'_> {
    fn as_ref(&self) -> InboxIdRef<'_> {
        self
    }
}

pub trait InboxOwner {
    /// Get address string of the wallet.
    fn get_identifier(&self) -> Result<Identifier, IdentifierValidationError>;

    /// Sign text with the wallet.
    fn sign(&self, text: &str) -> Result<UnverifiedSignature, SignatureError>;
}

impl InboxOwner for PrivateKeySigner {
    fn get_identifier(&self) -> Result<Identifier, IdentifierValidationError> {
        Identifier::eth(h160addr_to_string(self.address()))
    }

    fn sign(&self, text: &str) -> Result<UnverifiedSignature, SignatureError> {
        let signature_bytes = self.sign_message_sync(text.as_bytes())?;
        let sig = UnverifiedSignature::RecoverableEcdsa(UnverifiedRecoverableEcdsaSignature {
            signature_bytes: signature_bytes.into(),
        });
        Ok(sig)
    }
}

impl<T> InboxOwner for &T
where
    T: InboxOwner,
{
    fn get_identifier(&self) -> Result<Identifier, IdentifierValidationError> {
        (**self).get_identifier()
    }

    fn sign(&self, text: &str) -> Result<UnverifiedSignature, SignatureError> {
        (**self).sign(text)
    }
}
