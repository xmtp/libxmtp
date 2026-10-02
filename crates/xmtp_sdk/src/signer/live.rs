use crate::foreign;
use alloy::signers::local::PrivateKeySigner;
use std::{collections::HashMap, sync::Arc};
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_id::{InboxOwner, associations::unverified::UnverifiedSignature};

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
