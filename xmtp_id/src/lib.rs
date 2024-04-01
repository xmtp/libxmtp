mod credential;
pub mod error;
mod verified_key_package;

use std::sync::RwLock;

use openmls::prelude::Credential as OpenMlsCredential;
use openmls_basic_credential::SignatureKeyPair;
use prost::Message;
use xmtp_mls::{
    configuration::CIPHERSUITE, credential::Credential,
    credential::UnsignedGrantMessagingAccessData, types::Address, utils::time::now_ns,
};
use xmtp_proto::xmtp::mls::message_contents::MlsCredential as CredentialProto;

use crate::{
    credential::{CredentialVerifier, VerificationRequest, VerifiedCredential},
    error::IdentityError,
};

pub struct Identity {
    #[allow(dead_code)]
    pub(crate) account_address: Address,
    #[allow(dead_code)]
    pub(crate) installation_keys: SignatureKeyPair,
    pub(crate) credential: RwLock<Option<OpenMlsCredential>>,
    pub(crate) unsigned_association_data: Option<UnsignedGrantMessagingAccessData>,
}

impl Identity {
    // Creates a credential that is not yet wallet signed. Implementors should sign the payload returned by 'text_to_sign'
    // and call 'register' with the signature.
    #[allow(dead_code)]
    pub(crate) fn create_to_be_signed(account_address: String) -> Result<Self, IdentityError> {
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        let unsigned_association_data = UnsignedGrantMessagingAccessData::new(
            account_address.clone(),
            signature_keys.to_public_vec(),
            now_ns() as u64,
        )?;
        let identity = Self {
            account_address,
            installation_keys: signature_keys,
            credential: RwLock::new(None),
            unsigned_association_data: Some(unsigned_association_data),
        };

        Ok(identity)
    }

    pub fn text_to_sign(&self) -> Option<String> {
        if self.credential().is_ok() {
            return None;
        }
        self.unsigned_association_data
            .clone()
            .map(|data| data.text())
    }

    fn credential(&self) -> Result<OpenMlsCredential, IdentityError> {
        self.credential
            .read()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
            .ok_or(IdentityError::UninitializedIdentity)
    }

    pub(crate) async fn get_validated_account_address(
        credential: &[u8],
        installation_public_key: &[u8],
    ) -> Result<String, IdentityError> {
        let request = VerificationRequest::new(credential, installation_public_key);
        let credential = <Credential as CredentialVerifier>::verify_credential(request).await?;
        Ok(credential.account_address())
    }
}

#[cfg(test)]
mod tests {}
