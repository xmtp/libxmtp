use crate::{InboxId, XmtpError};
use xmtp_id::associations::{Identifier, ident};

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
        .map_err(XmtpError::from_core)
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

/// Calculate an inbox ID from a public identity and a nonce.
/// An omitted nonce is 1. This operation does not use a backend or storage.
#[xmtp_macro::sdk_export(pure, default(nonce = None))]
pub fn generate_inbox_id(
    identity: PublicIdentity,
    nonce: Option<u64>,
) -> Result<InboxId, XmtpError> {
    let id = identity
        .to_core()
        .map_err(|_| XmtpError::invalid_argument("invalid public identity"))?
        .inbox_id(nonce.unwrap_or(1))
        .map_err(|_| XmtpError::invalid_argument("invalid public identity"))?;
    InboxId::try_from(id)
}

#[cfg(not(feature = "pure-only"))]
include!("signer/live.rs");

#[cfg(test)]
mod pure_identity_tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn pure_inbox_calculation_matches_fixed_core_vectors() -> Result<(), XmtpError> {
        for (identifier, kind, nonce, expected) in [
            (
                "0xabcdef0000000000000000000000000000000000",
                PublicIdentityKind::Ethereum,
                0,
                "139a684d70154ab320b846179e5219b6e2d192048577779b230763a85a28365d",
            ),
            (
                "0xabcdef0000000000000000000000000000000000",
                PublicIdentityKind::Ethereum,
                1,
                "f020cf771dabaf2610250b5f00076215a8f1da8649ba46cf5ba2d00df6ce5279",
            ),
            (
                "0xabcdef0000000000000000000000000000000000",
                PublicIdentityKind::Ethereum,
                9007199254740993,
                "7388e86684247cde39d20ef985b6999cb657325d1576c28b74670a5392228913",
            ),
            (
                "0xabcdef0000000000000000000000000000000000",
                PublicIdentityKind::Ethereum,
                18446744073709551615,
                "00b23df9cd0b16c488b19647e02ee5872a8c7de14d056b937d1cd1f54c4a28fc",
            ),
            (
                "abcdef",
                PublicIdentityKind::Passkey,
                0,
                "e26bbe40a904acb658e0dd48f4031811b662ce4e6238eef5c46f5bb92550713a",
            ),
            (
                "abcdef",
                PublicIdentityKind::Passkey,
                1,
                "ac9f830ae6cf2299ba293dd4cec3be0d87a88e6a8fbfe5015de6fffd11d79b6e",
            ),
            (
                "abcdef",
                PublicIdentityKind::Passkey,
                9007199254740993,
                "ef91da01728a1d16593d300a7a699d6a7831c43e00916a6b199f8244f207637d",
            ),
            (
                "abcdef",
                PublicIdentityKind::Passkey,
                18446744073709551615,
                "469fa9bb87114e117a27305350728cb2a4f85fdae9b5ccaef3b702f879dd9ae4",
            ),
        ] {
            let identity = PublicIdentity {
                identifier: identifier.into(),
                kind,
            };
            assert_eq!(
                generate_inbox_id(identity.clone(), Some(nonce))?.into_checked()?,
                expected
            );
            if nonce == 1 {
                assert_eq!(generate_inbox_id(identity, None)?.into_checked()?, expected);
            }
        }
        Ok(())
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn pure_inbox_calculation_keeps_identity_validation_and_full_nonce() -> Result<(), XmtpError> {
        for identity in [
            PublicIdentity {
                identifier: "0xabcdef0000000000000000000000000000000000".into(),
                kind: PublicIdentityKind::Ethereum,
            },
            PublicIdentity {
                identifier: "abcdef".into(),
                kind: PublicIdentityKind::Passkey,
            },
        ] {
            let core = identity.to_core()?;
            assert_eq!(
                generate_inbox_id(identity.clone(), None)?.into_checked()?,
                core.inbox_id(1).map_err(XmtpError::from_core)?
            );
            for nonce in [0, 1, 9_007_199_254_740_993, u64::MAX] {
                assert_eq!(
                    generate_inbox_id(identity.clone(), Some(nonce))?.into_checked()?,
                    core.inbox_id(nonce).map_err(XmtpError::from_core)?
                );
            }
            assert_ne!(
                generate_inbox_id(identity.clone(), Some(0))?.into_checked()?,
                generate_inbox_id(identity, None)?.into_checked()?
            );
        }
        for (identifier, kind) in [
            ("invalid", PublicIdentityKind::Ethereum),
            ("not hex", PublicIdentityKind::Passkey),
        ] {
            assert!(matches!(
                generate_inbox_id(
                    PublicIdentity {
                        identifier: identifier.into(),
                        kind
                    },
                    None
                ),
                Err(XmtpError::InvalidArgument(_))
            ));
        }
        Ok(())
    }
}
