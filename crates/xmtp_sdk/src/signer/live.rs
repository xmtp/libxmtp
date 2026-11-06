use crate::{SecretBytes, SecretString, foreign};
use alloy::signers::local::PrivateKeySigner;
use std::{collections::HashMap, sync::Arc};
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_cryptography::secret::Zeroizing;
use xmtp_id::{
    InboxOwner,
    associations::{handle::Handle, unverified::UnverifiedSignature},
};
use xmtp_mls::inbox_handles::IdentityTarget;

/// A handle as a user typed it: ASCII capitals become lowercase. Text outside
/// the handle grammar fails with `InvalidArgument`, before any request. No
/// other rule applies, so a name shorter than the deployment's minimum
/// length is accepted.
// implements: CONF-081
pub(crate) fn handle_input(input: &str) -> Result<Handle, XmtpError> {
    Handle::from_input(input).map_err(|_| XmtpError::invalid_argument("invalid handle"))
}

impl PublicIdentity {
    /// The core target of this identity: an account or a handle, which only
    /// the verified resolver turns into an inbox.
    pub(crate) fn to_target(&self) -> Result<IdentityTarget, XmtpError> {
        match self.kind {
            PublicIdentityKind::Handle => {
                Ok(IdentityTarget::Handle(handle_input(&self.identifier)?))
            }
            PublicIdentityKind::Ethereum | PublicIdentityKind::Passkey => {
                Ok(IdentityTarget::Account(self.to_core()?))
            }
        }
    }
}

/// The targets of `identities`, in order. The first invalid identity fails
/// the whole list, before any request.
pub(crate) fn targets(identities: &[PublicIdentity]) -> Result<Vec<IdentityTarget>, XmtpError> {
    identities.iter().map(PublicIdentity::to_target).collect()
}

/// Split `targets` into accounts and handles, each in its original order.
pub(crate) fn split_targets(targets: Vec<IdentityTarget>) -> (Vec<Identifier>, Vec<Handle>) {
    let mut accounts = Vec::new();
    let mut handles = Vec::new();
    for target in targets {
        match target {
            IdentityTarget::Account(identifier) => accounts.push(identifier),
            IdentityTarget::Handle(handle) => handles.push(handle),
        }
    }
    (accounts, handles)
}

/// `can_message` result entries for handles, keyed `handle:<canonical handle>`.
pub(crate) fn handle_results(
    results: impl IntoIterator<Item = (Handle, bool)>,
) -> HashMap<String, bool> {
    results
        .into_iter()
        .map(|(handle, available)| (format!("handle:{handle}"), available))
        .collect()
}

pub(crate) fn can_message_results(
    results: impl IntoIterator<Item = (Identifier, bool)>,
) -> HashMap<String, bool> {
    results
        .into_iter()
        .map(|(identity, available)| {
            let key = match identity {
                Identifier::Ethereum(ident::Ethereum(text)) => format!("ethereum:{text}"),
                Identifier::Passkey(ident::Passkey { key, .. }) => {
                    format!("passkey:{}", hex::encode(key))
                }
            };
            (key, available)
        })
        .collect()
}

#[cfg(test)]
mod can_message_tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn result_keys_keep_identity_kind_and_core_text() -> Result<(), XmtpError> {
        let same_text = "1111111111111111111111111111111111111111";
        let eth = PublicIdentity {
            identifier: same_text.into(),
            kind: PublicIdentityKind::Ethereum,
        }
        .to_core()?;
        let passkey = PublicIdentity {
            identifier: same_text.into(),
            kind: PublicIdentityKind::Passkey,
        }
        .to_core()?;
        for (first, second) in [(true, false), (false, true)] {
            for results in [
                vec![(eth.clone(), first), (passkey.clone(), second)],
                vec![(passkey.clone(), second), (eth.clone(), first)],
            ] {
                let keys = can_message_results(results);
                assert_eq!(keys.len(), 2);
                assert_eq!(
                    keys["ethereum:1111111111111111111111111111111111111111"],
                    first
                );
                assert_eq!(
                    keys["passkey:1111111111111111111111111111111111111111"],
                    second
                );
            }
        }
        let prefixed = PublicIdentity {
            identifier: "0xABCDEF0000000000000000000000000000000000".into(),
            kind: PublicIdentityKind::Ethereum,
        }
        .to_core()?;
        let upper_passkey = PublicIdentity {
            identifier: "ABCDEF".into(),
            kind: PublicIdentityKind::Passkey,
        }
        .to_core()?;
        let keys = can_message_results([(prefixed, true), (upper_passkey, false)]);
        assert!(keys["ethereum:0xabcdef0000000000000000000000000000000000"]);
        assert!(!keys["passkey:abcdef"]);
        Ok(())
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

include!("local.rs");

#[xmtp_common::async_trait]
impl Signer for LocalSigningKey {
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
    generate_keys().await
}

#[xmtp_macro::sdk_export]
pub async fn local_signer_from_private_key(key: SecretBytes) -> Result<Arc<dyn Signer>, XmtpError> {
    Ok(LocalSigningKey::parse_bytes(&key)?)
}

/// The account a signer signs for. A signer that names a handle fails with
/// `InvalidArgument`: a handle signs nothing.
pub(crate) async fn identity(
    signer: std::sync::Arc<dyn Signer>,
) -> Result<PublicIdentity, XmtpError> {
    let identity = foreign::call(async move { signer.identity().await })
        .await
        .map_err(XmtpError::unknown)?
        .map_err(|_| XmtpError::signer())?;
    if matches!(identity.kind, PublicIdentityKind::Handle) {
        return Err(XmtpError::invalid_argument(
            "a signer identity must be an account, not a handle",
        ));
    }
    Ok(identity)
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
