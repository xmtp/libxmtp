mod grant_messaging_access;
mod legacy_create_identity;

use crate::{types::Address, InboxOwner};
use chrono::Utc;

use openmls_basic_credential::SignatureKeyPair;

use thiserror::Error;
use xmtp_cryptography::signature::SignatureError;

use xmtp_proto::xmtp::mls::message_contents::{
    mls_credential::Association as AssociationProto, MlsCredential as MlsCredentialProto,
};

use self::grant_messaging_access::GrantMessagingAccessAssociation;
use self::legacy_create_identity::LegacyCreateIdentityAssociation;

#[derive(Debug, Error)]
pub enum AssociationError {
    #[error("bad signature")]
    BadSignature(#[from] SignatureError),
    #[error("bad legacy signature: {0}")]
    BadLegacySignature(String),
    #[error("Association text mismatch")]
    TextMismatch,
    #[error("Installation public key mismatch")]
    InstallationPublicKeyMismatch,
    #[error(
        "Address mismatch in Association: Provided:{provided_addr:?} != signed:{signing_addr:?}"
    )]
    AddressMismatch {
        provided_addr: Address,
        signing_addr: Address,
    },
    #[error("Malformed association")]
    MalformedAssociation,
}

pub enum Credential {
    GrantMessagingAccess(GrantMessagingAccessAssociation),
    LegacyCreateIdentity(LegacyCreateIdentityAssociation),
}

impl Credential {
    pub fn create(
        installation_keys: &SignatureKeyPair,
        owner: &impl InboxOwner,
    ) -> Result<Self, AssociationError> {
        let iso8601_time = format!("{}", Utc::now().format("%+"));
        let association = GrantMessagingAccessAssociation::create(
            owner,
            installation_keys.to_public_vec(),
            iso8601_time,
        )?;
        Ok(Self::GrantMessagingAccess(association))
    }

    pub fn create_legacy() -> Result<Self, AssociationError> {
        todo!()
    }

    pub fn from_proto_validated(
        proto: MlsCredentialProto,
        expected_account_address: Option<&str>, // Must validate when fetching identity updates
        expected_installation_public_key: Option<&[u8]>, // Must cross-reference against leaf node when relevant
    ) -> Result<Self, AssociationError> {
        let credential = match proto
            .association
            .ok_or(AssociationError::MalformedAssociation)?
        {
            AssociationProto::MessagingAccess(assoc) => {
                GrantMessagingAccessAssociation::from_proto_validated(
                    assoc,
                    &proto.installation_public_key,
                )
                .map(Credential::GrantMessagingAccess)
            }
            AssociationProto::LegacyCreateIdentity(assoc) => todo!(),
        }?;
        if let Some(address) = expected_account_address {
            if credential.address() != address {
                return Err(AssociationError::AddressMismatch {
                    provided_addr: address.to_string(),
                    signing_addr: credential.address(),
                });
            }
        }
        if let Some(public_key) = expected_installation_public_key {
            if credential.installation_public_key() != public_key {
                return Err(AssociationError::InstallationPublicKeyMismatch);
            }
        }
        Ok(credential)
    }

    pub fn address(&self) -> String {
        match &self {
            Credential::GrantMessagingAccess(assoc) => assoc.address(),
            Credential::LegacyCreateIdentity(assoc) => assoc.address(),
        }
    }

    pub fn installation_public_key(&self) -> Vec<u8> {
        match &self {
            Credential::GrantMessagingAccess(assoc) => assoc.installation_public_key(),
            Credential::LegacyCreateIdentity(assoc) => assoc.installation_public_key(),
        }
    }

    pub fn iso8601_time(&self) -> String {
        match &self {
            Credential::GrantMessagingAccess(assoc) => assoc.iso8601_time(),
            Credential::LegacyCreateIdentity(assoc) => assoc.iso8601_time(),
        }
    }
}

impl From<Credential> for MlsCredentialProto {
    fn from(credential: Credential) -> Self {
        Self {
            installation_public_key: credential.installation_public_key(),
            association: match credential {
                Credential::GrantMessagingAccess(assoc) => {
                    Some(AssociationProto::MessagingAccess(assoc.into()))
                }
                Credential::LegacyCreateIdentity(assoc) => todo!(),
            },
        }
    }
}

#[cfg(test)]
pub mod tests {
    use ethers::signers::{LocalWallet, Signer};
    use xmtp_cryptography::{signature::h160addr_to_string, utils::rng};
    use xmtp_proto::xmtp::mls::message_contents::MessagingAccessAssociation as MessagingAccessAssociationProto;

    use crate::association::AssociationContext;

    use super::{AssociationData, Eip191Association};

    #[tokio::test]
    async fn assoc_gen() {
        let key_bytes = vec![22, 33, 44, 55];

        let wallet = LocalWallet::new(&mut rng());
        let other_wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());
        let other_addr = h160addr_to_string(other_wallet.address());
        let grant_time = "2021-01-01T00:00:00Z";
        let bad_grant_time = "2021-01-01T00:00:01Z";
        let text = AssociationData::new_grant_messaging_access(
            addr.clone(),
            key_bytes.clone(),
            grant_time.to_string(),
        );
        let sig = wallet.sign_message(text.text()).await.expect("BadSign");

        let bad_key_bytes = vec![11, 22, 33];
        let bad_text1 = AssociationData::new_grant_messaging_access(
            addr.clone(),
            bad_key_bytes.clone(),
            grant_time.to_string(),
        );
        let bad_text2 = AssociationData::new_grant_messaging_access(
            other_addr.clone(),
            key_bytes.clone(),
            grant_time.to_string(),
        );
        let bad_text3 = AssociationData::new_grant_messaging_access(
            addr.clone(),
            key_bytes.clone(),
            bad_grant_time.to_string(),
        );
        let bad_text4 = AssociationData::new_grant_messaging_access(
            addr.clone(),
            key_bytes.clone(),
            grant_time.to_string(),
        );
        let other_text = AssociationData::new_grant_messaging_access(
            other_addr.clone(),
            key_bytes.clone(),
            grant_time.to_string(),
        );

        let other_sig = wallet
            .sign_message(other_text.text())
            .await
            .expect("BadSign");

        assert!(Eip191Association::new_validated(text.clone(), sig.into()).is_ok());
        assert!(Eip191Association::new_validated(bad_text1.clone(), sig.into()).is_err());
        assert!(Eip191Association::new_validated(bad_text2.clone(), sig.into()).is_err());
        assert!(Eip191Association::new_validated(bad_text3.clone(), sig.into()).is_err());
        assert!(Eip191Association::new_validated(bad_text4.clone(), sig.into()).is_err());
        assert!(Eip191Association::new_validated(text.clone(), other_sig.into()).is_err());
    }

    #[tokio::test]
    async fn to_proto() {
        let key_bytes = vec![22, 33, 44, 55];
        let wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());
        let text = AssociationData::new_grant_messaging_access(
            addr.clone(),
            key_bytes.clone(),
            "2021-01-01T00:00:00Z".to_string(),
        );
        let sig = wallet.sign_message(text.text()).await.expect("BadSign");

        let assoc = Eip191Association::new_validated(text.clone(), sig.into()).unwrap();
        let proto_signature: MessagingAccessAssociationProto = assoc.into();

        assert_eq!(proto_signature.association_text_version, 1);
        assert_eq!(proto_signature.signature.unwrap().bytes, sig.to_vec());
    }
}
