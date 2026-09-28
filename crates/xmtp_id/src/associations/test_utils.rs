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
use prost::Message;
use xmtp_common::rand_vec;
use xmtp_cryptography::CredentialSign;
use xmtp_cryptography::basic_credential::XmtpInstallationCredential;
use xmtp_proto::xmtp::identity::associations::{
    CreateInbox as CreateInboxProto, IdentityUpdate as IdentityUpdateProto,
    RecoverableEcdsaSignature,
};

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

/// A genuine legacy `SignedPublicKey` for the wallet
/// `0x220ca99fb7fafa18cb623d924794dde47b4bc2e9`, once accepted as the
/// delegated key of a `delegated_erc_191` signature.
const LEGACY_SIGNED_PUBLIC_KEY: &[u8] = &[
    10, 79, 8, 192, 195, 165, 174, 203, 153, 231, 213, 23, 26, 67, 10, 65, 4, 216, 84, 174, 252,
    198, 225, 219, 168, 239, 166, 62, 233, 206, 108, 53, 155, 87, 132, 8, 43, 91, 36, 91, 81, 93,
    213, 67, 241, 69, 5, 31, 249, 186, 129, 119, 144, 4, 44, 54, 76, 185, 95, 61, 23, 231, 72, 7,
    169, 18, 70, 113, 79, 173, 82, 13, 37, 146, 201, 43, 174, 180, 33, 125, 43, 18, 70, 18, 68, 10,
    64, 7, 136, 100, 172, 155, 247, 230, 255, 253, 247, 78, 50, 212, 226, 41, 78, 239, 183, 136,
    247, 122, 88, 155, 245, 219, 183, 215, 202, 42, 89, 162, 128, 96, 96, 120, 131, 17, 70, 38,
    231, 2, 27, 91, 29, 66, 110, 128, 140, 1, 42, 217, 185, 2, 181, 208, 100, 143, 143, 219, 159,
    174, 1, 233, 191, 16, 1,
];
pub const LEGACY_WALLET: &str = "0x220ca99fb7fafa18cb623d924794dde47b4bc2e9";

/// Encodes `body` as length-delimited field `tag`, so a test can write a
/// field the generated types no longer have.
pub fn length_delimited(tag: u32, body: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    prost::encoding::bytes::encode(tag, &body.to_vec(), &mut buf);
    buf
}

/// A `Signature` carrying the retired oneof field 4, shaped exactly as the
/// legacy delegated signature was: the delegated key, then the signature.
pub fn retired_signature() -> Vec<u8> {
    let delegated = [
        length_delimited(1, LEGACY_SIGNED_PUBLIC_KEY),
        length_delimited(
            2,
            &RecoverableEcdsaSignature {
                bytes: rand_vec::<65>(),
            }
            .encode_to_vec(),
        ),
    ]
    .concat();
    length_delimited(4, &delegated)
}

/// An encoded `IdentityUpdate` for `inbox_id` holding the encoded `actions`,
/// which may carry fields the generated types no longer have.
pub fn identity_update(inbox_id: &str, actions: Vec<Vec<u8>>) -> Vec<u8> {
    let head = IdentityUpdateProto {
        actions: vec![],
        client_timestamp_ns: 1,
        inbox_id: inbox_id.to_string(),
    };
    [head.encode_to_vec()]
        .into_iter()
        .chain(actions.iter().map(|action| length_delimited(1, action)))
        .collect::<Vec<_>>()
        .concat()
}

/// An encoded `IdentityUpdate` creating `inbox_id` for the Ethereum
/// `identifier`, signed with the retired field 4.
pub fn retired_signature_create_inbox(inbox_id: &str, identifier: &str) -> Vec<u8> {
    let create = [
        CreateInboxProto {
            initial_identifier: identifier.to_string(),
            ..Default::default()
        }
        .encode_to_vec(),
        length_delimited(3, &retired_signature()),
    ]
    .concat();
    identity_update(inbox_id, vec![length_delimited(1, &create)])
}
