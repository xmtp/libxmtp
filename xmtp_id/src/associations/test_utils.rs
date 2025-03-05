#![allow(clippy::unwrap_used)]
use super::{
    builder::SignatureRequest,
    member::Identifier,
    unsigned_actions::UnsignedCreateInbox,
    unverified::{UnverifiedAction, UnverifiedCreateInbox, UnverifiedSignature},
    AccountId, InstallationKeyContext, MemberIdentifier,
};
use crate::{
    scw_verifier::{SmartContractSignatureVerifier, ValidationResponse, VerifierError},
    InboxOwner,
};
use ethers::{
    core::types::BlockNumber,
    signers::{LocalWallet, Signer},
    types::Bytes,
};
use xmtp_cryptography::basic_credential::XmtpInstallationCredential;
use xmtp_cryptography::CredentialSign;

#[derive(Debug, Clone)]
pub struct MockSmartContractSignatureVerifier {
    is_valid_signature: bool,
}

impl MockSmartContractSignatureVerifier {
    pub fn new(is_valid_signature: bool) -> Self {
        Self { is_valid_signature }
    }
}

pub trait WalletTestExt {
    fn get_inbox_id(&self, nonce: u64) -> String;
    fn member_identifier(&self) -> MemberIdentifier;
    fn identifier(&self) -> Identifier;
}

impl WalletTestExt for LocalWallet {
    fn get_inbox_id(&self, nonce: u64) -> String {
        self.identifier().inbox_id(nonce).unwrap()
    }
    fn member_identifier(&self) -> MemberIdentifier {
        self.identifier().into()
    }
    fn identifier(&self) -> Identifier {
        self.get_identifier().unwrap()
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl SmartContractSignatureVerifier for MockSmartContractSignatureVerifier {
    async fn is_valid_signature(
        &self,
        _account_id: AccountId,
        _hash: [u8; 32],
        _signature: Bytes,
        _block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        Ok(ValidationResponse {
            is_valid: self.is_valid_signature,
            block_number: Some(1),
            error: None,
        })
    }
}

pub async fn add_wallet_signature(signature_request: &mut SignatureRequest, wallet: &LocalWallet) {
    let signature_text = signature_request.signature_text();
    let sig = wallet.sign_message(signature_text).await.unwrap().to_vec();
    let unverified_sig = UnverifiedSignature::new_recoverable_ecdsa(sig);
    let scw_verifier = MockSmartContractSignatureVerifier::new(false);

    signature_request
        .add_signature(unverified_sig, &scw_verifier)
        .await
        .expect("should succeed");
}

pub async fn add_installation_key_signature(
    signature_request: &mut SignatureRequest,
    installation_key: &XmtpInstallationCredential,
) {
    let signature_text = signature_request.signature_text();
    let sig = installation_key
        .credential_sign::<InstallationKeyContext>(signature_text)
        .unwrap();

    let unverified_sig =
        UnverifiedSignature::new_installation_key(sig, installation_key.verifying_key());

    signature_request
        .add_signature(
            unverified_sig,
            &MockSmartContractSignatureVerifier::new(false),
        )
        .await
        .expect("should succeed");
}

impl UnverifiedAction {
    pub fn new_test_create_inbox(account_address: &str, nonce: &u64) -> Self {
        Self::CreateInbox(UnverifiedCreateInbox::new(
            UnsignedCreateInbox {
                account_identifier: Identifier::eth(account_address)
                    .expect("test account address is invalid"),
                nonce: *nonce,
            },
            UnverifiedSignature::new_recoverable_ecdsa(vec![1, 2, 3]),
        ))
    }
}
