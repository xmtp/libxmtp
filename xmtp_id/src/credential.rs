use prost::Message;
use xmtp_mls::{
    credential::{Credential, GrantMessagingAccessAssociation, LegacyCreateIdentityAssociation},
    types::Address,
};
use xmtp_proto::xmtp::mls::message_contents::MlsCredential as CredentialProto;
#[derive(Debug, Clone)]
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
    installation_public_key: Vec<u8>,
    credential: Vec<u8>,
}

impl VerificationRequest {
    pub fn new(installation_public_key: Vec<u8>, credential: Vec<u8>) -> Self {
        Self {
            installation_public_key,
            credential,
        }
    }
}

type VerificationResult = Result<VerifiedCredential, VerificationError>;

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

#[async_trait::async_trait]
impl CredentialVerifier for Credential {
    async fn verify_credential(request: VerificationRequest) -> VerificationResult {
        let proto = CredentialProto::decode(request.credential);
        let credential =
            Credential::from_proto_validated(proto, None, Some(request.installation_public_key));
        Ok(match credential {
            Credential::GrantMessagingAccess(cred) => VerifiedCredential {
                account_address: cred.account_address(),
                account_type: AssociationType::ExternallyOwned,
            },
            Credential::LegacyCreateIdentity(cred) => VerifiedCredential {
                account_address: cred.account_address(),
                account_type: AssociationType::Legacy,
            },
        })
    }
}
