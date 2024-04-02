use prost::Message;
use xmtp_mls::{
    credential::{AssociationError, Credential},
    types::Address,
};
use xmtp_proto::xmtp::mls::message_contents::MlsCredential as CredentialProto;

#[derive(Debug, Clone)]
pub enum AssociationType {
    ExternallyOwned,
    #[allow(dead_code)]
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
    #[error("protobuf deserialization: {0}")]
    Deserialization(#[from] prost::DecodeError),
    #[error("bad association: {0}")]
    BadAssocation(#[from] AssociationError),
}

#[derive(Debug, Clone)]
pub struct VerifiedCredential {
    pub account_address: String,
    pub account_type: AssociationType,
}

impl VerifiedCredential {
    pub fn account_address(&self) -> &String {
        &self.account_address
    }
}

#[derive(Debug, Clone)]
pub struct VerificationRequest {
    installation_public_key: Vec<u8>,
    credential: Vec<u8>,
}

impl VerificationRequest {
    pub fn new<I, C>(installation_public_key: I, credential: C) -> Self
    where
        I: AsRef<[u8]>,
        C: AsRef<[u8]>,
    {
        Self {
            installation_public_key: installation_public_key.as_ref().to_vec(),
            credential: credential.as_ref().to_vec(),
        }
    }
}

type VerificationResult = Result<VerifiedCredential, VerificationError>;

#[async_trait::async_trait]
pub trait CredentialVerifier {
    /// Verify a single MLS credential.
    async fn verify_credential(request: VerificationRequest) -> VerificationResult;
    /// Verify a batch of MLS credentials.
    /// Returns the results in the same order as provided.
    async fn batch_verify_credentials(
        credentials_to_verify: Vec<VerificationRequest>,
    ) -> Vec<VerificationResult> {
        let results = credentials_to_verify
            .into_iter()
            .map(Self::verify_credential);

        futures::future::join_all(results).await
    }
}

#[async_trait::async_trait]
impl CredentialVerifier for Credential {
    async fn verify_credential(request: VerificationRequest) -> VerificationResult {
        let proto = CredentialProto::decode(request.credential.as_slice())?;
        let credential = Credential::from_proto_validated(
            proto,
            None,
            Some(request.installation_public_key.as_slice()),
        )?;
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

#[cfg(test)]
mod test {
    use super::*;

    /// Extremely basic credential that is either valid or not.
    struct TestCredential {
        pub valid: bool,
    }

    impl TestCredential {
        pub fn new(valid: bool) -> Self {
            Self { valid }
        }
    }

    impl From<TestCredential> for Vec<u8> {
        fn from(cred: TestCredential) -> Vec<u8> {
            if cred.valid {
                vec![0]
            } else {
                vec![1]
            }
        }
    }

    impl From<Vec<u8>> for TestCredential {
        fn from(bytes: Vec<u8>) -> Self {
            Self {
                valid: bytes[0] == 0,
            }
        }
    }

    #[async_trait::async_trait]
    impl CredentialVerifier for TestCredential {
        async fn verify_credential(request: VerificationRequest) -> VerificationResult {
            let cred = TestCredential::from(request.credential);
            if cred.valid {
                Ok(VerifiedCredential {
                    account_address: "test".to_string(),
                    account_type: AssociationType::ExternallyOwned,
                })
            } else {
                Err(VerificationError::BadAssocation(
                    AssociationError::TextMismatch,
                ))
            }
        }
    }

    #[test]
    fn test_batch_verify() {
        let requests = vec![
            VerificationRequest::new(vec![0], vec![0]),
            VerificationRequest::new(vec![0], vec![1]),
            VerificationRequest::new(vec![0], vec![0]),
        ];
        let results =
            futures::executor::block_on(TestCredential::batch_verify_credentials(requests));
        assert!(matches!(results[0], Ok(_)));
        assert!(matches!(results[1], Err(_)));
        assert!(matches!(results[2], Ok(_)));
    }
}
