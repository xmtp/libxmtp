use super::{
    builder::SignatureRequest,
    unsigned_actions::UnsignedCreateInbox,
    unverified::{UnverifiedAction, UnverifiedCreateInbox, UnverifiedSignature},
    AccountId,
};
use crate::{
    constants::INSTALLATION_KEY_SIGNATURE_CONTEXT,
    scw_verifier::{SmartContractSignatureVerifier, VerifierError},
};
use ed25519_dalek::SigningKey as Ed25519SigningKey;
use ethers::{
    core::types::BlockNumber,
    signers::{LocalWallet, Signer},
    types::{Bytes, U64},
};
use rand::{distributions::Alphanumeric, Rng};
use sha2::{Digest, Sha512};

pub fn rand_string() -> String {
    let v: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    v.to_lowercase()
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

#[async_trait::async_trait]
impl SmartContractSignatureVerifier for MockSmartContractSignatureVerifier {
    async fn is_valid_signature(
        &self,
        _account_id: AccountId,
        _hash: [u8; 32],
        _signature: Bytes,
        _block_number: Option<BlockNumber>,
    ) -> Result<bool, VerifierError> {
        Ok(self.is_valid_signature)
    }

    async fn current_block_number(&self, _chain_id: &str) -> Result<U64, VerifierError> {
        Ok(1.into())
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
    installation_key: &Ed25519SigningKey,
) {
    let signature_text = signature_request.signature_text();
    let verifying_key = installation_key.verifying_key();
    let mut prehashed: Sha512 = Sha512::new();
    prehashed.update(signature_text);

    let sig = installation_key
        .sign_prehashed(prehashed, Some(INSTALLATION_KEY_SIGNATURE_CONTEXT))
        .unwrap();
    let unverified_sig = UnverifiedSignature::new_installation_key(
        sig.to_bytes().to_vec(),
        verifying_key.as_bytes().to_vec(),
    );

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
