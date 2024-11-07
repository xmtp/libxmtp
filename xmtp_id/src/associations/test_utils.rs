#![allow(clippy::unwrap_used)]
use super::{
    builder::SignatureRequest,
    unsigned_actions::UnsignedCreateInbox,
    unverified::{UnverifiedAction, UnverifiedCreateInbox, UnverifiedSignature},
    AccountId,
};
use crate::scw_verifier::{SmartContractSignatureVerifier, ValidationResponse, VerifierError};
use ethers::{
    core::types::BlockNumber,
    signers::{LocalWallet, Signer},
    types::Bytes,
};
use rand::Rng;
use xmtp_cryptography::basic_credential::XmtpInstallationCredential;
use xmtp_cryptography::CredentialSign;

pub fn rand_string() -> String {
    let hex_chars = "0123456789abcdef";
    let v: String = (0..40)
        .map(|_| {
            let idx = rand::thread_rng().gen_range(0..hex_chars.len());
            hex_chars.chars().nth(idx).unwrap()
        })
        .collect();

    format!("0x{}", v)
}

pub fn rand_u64() -> u64 {
    rand::thread_rng().gen()
}

pub fn rand_vec() -> Vec<u8> {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill(&mut buf[..]);
    buf.to_vec()
}

#[derive(Debug, Clone)]
pub struct MockSmartContractSignatureVerifier {
    is_valid_signature: bool,
}

impl MockSmartContractSignatureVerifier {
    pub fn new(is_valid_signature: bool) -> Self {
        Self { is_valid_signature }
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
    let sig = installation_key.credential_sign(signature_text).unwrap();

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
                account_address: account_address.to_owned(),
                nonce: *nonce,
            },
            UnverifiedSignature::new_recoverable_ecdsa(vec![1, 2, 3]),
        ))
    }
}
