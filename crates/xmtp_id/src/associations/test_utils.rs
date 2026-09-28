#![allow(clippy::unwrap_used)]
use super::{
    AccountId, InstallationKeyContext, MemberIdentifier,
    builder::SignatureRequest,
    member::Identifier,
    unsigned_actions::UnsignedCreateInbox,
    unverified::{UnverifiedAction, UnverifiedCreateInbox, UnverifiedSignature},
};
use crate::{
    InboxOwner,
    scw_verifier::{SmartContractSignatureVerifier, ValidationResponse, VerifierError},
};
use alloy::{
    primitives::{BlockNumber, Bytes},
    signers::{Signer, k256::ecdsa::Signature as K256Signature, local::PrivateKeySigner},
};
use p256::ecdsa::Signature as P256Signature;
use xmtp_cryptography::CredentialSign;
use xmtp_cryptography::basic_credential::XmtpInstallationCredential;

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

impl WalletTestExt for PrivateKeySigner {
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

#[xmtp_common::async_trait]
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

/// The same 65-byte secp256k1 signature with its recovery byte moved between the 0/1 and 27/28
/// forms.
pub fn ecdsa_recovery_byte_alias(signature: &[u8]) -> Vec<u8> {
    let (rs, v) = signature.split_at(64);
    let v = if v[0] < 27 { v[0] + 27 } else { v[0] - 27 };
    [rs, &[v]].concat()
}

/// The same 65-byte secp256k1 signature with `s` negated to the other half of the curve order and
/// the recovery parity flipped to match, keeping the recovery byte's form.
pub fn ecdsa_negated_s_alias(signature: &[u8]) -> Vec<u8> {
    let (rs, v) = signature.split_at(64);
    let (r, s) = K256Signature::from_slice(rs).unwrap().split_scalars();
    let negated = K256Signature::from_scalars(r, -s).unwrap();
    let v = if v[0] < 27 { 1 - v[0] } else { 55 - v[0] };
    [negated.to_bytes().as_slice(), &[v]].concat()
}

/// The same DER-encoded P-256 signature with `s` negated to the other half of the curve order.
pub fn p256_negated_s_alias(der: &[u8]) -> Vec<u8> {
    let (r, s) = P256Signature::from_der(der).unwrap().split_scalars();
    let negated = P256Signature::from_scalars(r, -s).unwrap();
    negated.to_der().as_bytes().to_vec()
}

pub async fn add_wallet_signature(
    signature_request: &mut SignatureRequest,
    wallet: &PrivateKeySigner,
) {
    let signature_text = signature_request.signature_text();
    let sig = wallet
        .sign_message(signature_text.as_bytes())
        .await
        .unwrap();
    let unverified_sig = UnverifiedSignature::new_recoverable_ecdsa(sig.into());
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
