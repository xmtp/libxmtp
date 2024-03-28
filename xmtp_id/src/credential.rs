use prost::Message;
use xmtp_mls::{
    credential::{GrantMessagingAccessAssociation, LegacyCreateIdentityAssociation},
    types::Address,
};
use xmtp_proto::xmtp::mls::message_contents::MlsCredential as CredentialProto;

pub enum AssociationType {
    ExternallyOwned,
    SmartContract,
    Legacy,
}

#[derive(thiserror::Error, Debug)]
pub enum VerificationError {
    #[error(
        "Address mismatch in Association: Provided:{provided_addr:?} != signed:{signing_addr:?}"
    )]
    AddressMismatch {
        provided_addr: Address,
        signing_addr: Address,
    },
    #[error("Installation public key mismatch")]
    InstallationPublicKeyMismatch,
}

pub struct VerifiedCredential {
    pub account_address: String,
    pub account_type: AssociationType,
}

impl VerifiedCredential {
    pub fn account_address(&self) -> String {
        self.account_address.clone()
    }

    pub fn account_type(&self) -> AssociationType {
        self.account_type.clone()
    }
}

pub struct VerificationRequest {
    expected_account_address: String,
    installation_public_key: Vec<u8>,
    credential: Vec<u8>,
}

type VerificationResult = Result<VerifiedCredential, VerificationError>;

pub trait Credential {
    fn address(&self) -> String;
    fn installation_public_key(&self) -> Vec<u8>;
    fn created_ns(&self) -> u64;
}

#[async_trait::async_trait]
pub trait CredentialVerifier {
    async fn verify_credential(request: VerificationRequest) -> VerificationResult;
    async fn batch_verify_credentials(
        credentials_to_verify: Vec<VerificationRequest>,
    ) -> Vec<VerificationResult> {
        let mut results = Vec::new();
        for credential in credentials_to_verify {
            results.push(Self::verify_credential(credential));
        }

        futures::future::join_all(results).await
    }
}

impl<'a> Credential for &'a GrantMessagingAccessAssociation {
    fn address(&self) -> String {
        self.account_address().clone()
    }

    fn installation_public_key(&self) -> Vec<u8> {
        self.installation_public_key().clone()
    }

    fn created_ns(&self) -> u64 {
        GrantMessagingAccessAssociation::created_ns(self)
    }
}

fn validate_credential(
    credential: impl Credential,
    request: VerificationRequest,
) -> Result<(), VerificationError> {
    if credential.address() != request.expected_account_address {
        return Err(VerificationError::AddressMismatch {
            provided_addr: request.expected_account_address.to_string(),
            signing_addr: credential.address(),
        });
    }

    if credential.installation_public_key() != request.installation_public_key {
        return Err(VerificationError::InstallationPublicKeyMismatch);
    }

    Ok(())
}

#[async_trait::async_trait]
impl CredentialVerifier for GrantMessagingAccessAssociation {
    async fn verify_credential(request: VerificationRequest) -> VerificationResult {
        let proto = CredentialProto::decode(request.credential);
        let credential = GrantMessagingAccessAssociation::from_proto_validated(
            proto,
            Some(request.installation_public_key),
        );
        validate_credential(&credential, request)?;

        Ok(VerifiedCredential {
            account_address: credential.account_address(),
            account_type: AssociationType::EOA,
        })
    }
}

#[async_trait::async_trait]
impl CredentialVerifier for LegacyCreateIdentityAssociation {
    async fn verify_credential(request: VerificationRequest) -> VerificationResult {
        let proto = CredentialProto::decode(request.credential);
        let credential = LegacyCreateIdentityAssociation::from_proto_validated(
            proto,
            Some(request.installation_public_key),
        );
        validate_credential(&credential, request)?;

        Ok(VerifiedCredential {
            account_address: credential.account_address(),
            account_type: AssociationType::Legacy,
        })
    }
}
