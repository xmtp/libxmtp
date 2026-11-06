#![allow(dead_code)]
use super::{
    AccountId, HasMemberKind, InstallationKeyContext, MemberIdentifier, SignatureError,
    SignatureKind, ident,
};
use crate::scw_verifier::{ScwInput, SmartContractSignatureVerifier, ValidationResponse};
use alloy::primitives::Signature as EtherSignature;
use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use xmtp_cryptography::CredentialVerify;
use xmtp_cryptography::hash::sha256_bytes;
use xmtp_cryptography::signature::h160addr_to_string;

/// The domain of every replay key (see [`replay_key`]).
const REPLAY_KEY_DOMAIN: &str = "XMTP identity replay key v1";

/// The replay key of a signature by `signer` over `signature_text`: the SHA-256 digest of the
/// domain, the signer's member kind (`ethereum`, `installation`, or `passkey`), the signer's
/// identifier text (a passkey's without its relying party), and the signature text, joined by
/// line feeds. It never depends on the signature bytes, kind, or chain, so every encoding of one
/// approval, in any signature kind, yields one key.
// implements: IDENT-090
fn replay_key(signer: &MemberIdentifier, signature_text: &str) -> Vec<u8> {
    let preimage = format!(
        "{REPLAY_KEY_DOMAIN}\n{kind}\n{signer}\n{signature_text}",
        kind = signer.kind()
    );
    sha256_bytes(preimage.as_bytes())
}

#[derive(Debug, Clone)]
pub struct VerifiedSignature {
    pub signer: MemberIdentifier,
    pub kind: SignatureKind,
    /// The association-log replay key, derived from the signer and the signature text. Private to
    /// this module, so only [`VerifiedSignature::new_verified`] sets it outside tests.
    replay_key: Vec<u8>,
    pub chain_id: Option<u64>,
}

#[derive(serde::Deserialize)]
struct ClientDataJson {
    origin: String,
    challenge: String,
}

impl VerifiedSignature {
    /// A signature with an explicit replay key, for tests that build actions by hand.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new(
        signer: MemberIdentifier,
        kind: SignatureKind,
        replay_key: Vec<u8>,
        chain_id: Option<u64>,
    ) -> Self {
        Self {
            signer,
            kind,
            replay_key,
            chain_id,
        }
    }

    /// A signature that `signer` made over `signature_text`, with the replay key derived from
    /// both. The only production constructor.
    pub(crate) fn new_verified(
        signer: MemberIdentifier,
        kind: SignatureKind,
        signature_text: &str,
        chain_id: Option<u64>,
    ) -> Self {
        Self {
            replay_key: replay_key(&signer, signature_text),
            signer,
            kind,
            chain_id,
        }
    }

    /// The association-log replay key: the signer and the signature text.
    pub(crate) fn replay_key(&self) -> &[u8] {
        &self.replay_key
    }

    /// Recovers the signer of a 65-byte `r`, `s`, recovery secp256k1 signature over the
    /// signature text. The recovery byte must be 0, 1, 27, or 28, and `s` may be in either half
    /// of the curve order.
    pub fn from_recoverable_ecdsa<Text: AsRef<str>>(
        signature_text: Text,
        signature_bytes: &[u8],
    ) -> Result<Self, SignatureError> {
        let Ok(bytes @ [.., 0 | 1 | 27 | 28]) = <[u8; 65]>::try_from(signature_bytes) else {
            return Err(SignatureError::Invalid);
        };
        let signature = EtherSignature::from_raw_array(&bytes)?.normalized_s();
        let address = signature.recover_address_from_msg(signature_text.as_ref())?;

        Ok(Self::new_verified(
            MemberIdentifier::eth(h160addr_to_string(address))?,
            SignatureKind::Erc191,
            signature_text.as_ref(),
            None,
        ))
    }

    /**
     * Verifies an ECDSA signature against the provided signature text and ensures that the recovered
     * address matches the expected address.
     */
    pub fn from_recoverable_ecdsa_with_expected_address<Text: AsRef<str>>(
        signature_text: Text,
        signature_bytes: &[u8],
        expected_address: Text,
    ) -> Result<Self, SignatureError> {
        let partially_verified = Self::from_recoverable_ecdsa(signature_text, signature_bytes)?;
        if partially_verified
            .signer
            .eth_address()
            .ok_or(SignatureError::Invalid)?
            .to_lowercase()
            != expected_address.as_ref().to_lowercase()
        {
            return Err(SignatureError::Invalid);
        }

        Ok(partially_verified)
    }

    /**
     * Verifies an installation key signature against the provided signature text and verifying key bytes.
     * Returns a VerifiedSignature if the signature is valid, otherwise returns an error.
     */
    pub fn from_installation_key<Text: AsRef<str>>(
        signature_text: Text,
        signature_bytes: &[u8],
        verifying_key: ed25519_dalek::VerifyingKey,
    ) -> Result<Self, SignatureError> {
        verifying_key.credential_verify::<InstallationKeyContext>(
            signature_text.as_ref(),
            signature_bytes.try_into()?,
        )?;
        Ok(Self::new_verified(
            MemberIdentifier::installation(verifying_key.as_bytes().to_vec()),
            SignatureKind::InstallationKey,
            signature_text.as_ref(),
            None,
        ))
    }

    /// Verifies a WebAuthn P-256 assertion over the signature text. `s` may be in either half of
    /// the curve order.
    pub fn from_passkey<Text: AsRef<str>>(
        signature_text: Text,
        public_key: &[u8],
        signature: &[u8],
        authenticator_data: &[u8],
        client_data_json: &[u8],
    ) -> Result<Self, SignatureError> {
        let client_data: ClientDataJson = serde_json::from_slice(client_data_json)
            .map_err(|_| SignatureError::InvalidClientData)?;

        let challenge = BASE64_URL_SAFE_NO_PAD.encode(signature_text.as_ref());
        if challenge != client_data.challenge {
            // Challenge needs to match signature text
            return Err(SignatureError::InvalidClientData);
        }

        // 1. Parse the public key from raw bytes
        let verifying_key = VerifyingKey::from_sec1_bytes(public_key)
            .map_err(|_| SignatureError::InvalidPublicKey)?;

        // 2. Parse the signature
        let signature = Signature::from_der(signature).map_err(|_| SignatureError::Invalid)?;

        // 3. Hash the client data
        let client_data_hash = sha256_bytes(client_data_json);

        // 4. Construct the verification data (authenticator_data + client_data_hash)
        let mut verification_data =
            Vec::with_capacity(authenticator_data.len() + client_data_hash.len());
        verification_data.extend_from_slice(authenticator_data);
        verification_data.extend_from_slice(&client_data_hash);

        // 5. Verify the signature
        verifying_key.verify(&verification_data, &signature)?;

        Ok(Self::new_verified(
            MemberIdentifier::Passkey(ident::Passkey {
                key: public_key.to_vec(),
                relying_party: Some(client_data.origin),
            }),
            SignatureKind::P256,
            signature_text.as_ref(),
            None,
        ))
    }

    /// Verifies a smart contract wallet signature using the provided signature verifier. The
    /// input is checked before the verifier is called, and a valid verdict writes the block it
    /// was reached at to `block_number`.
    pub async fn from_smart_contract_wallet<Text: AsRef<str>>(
        signature_text: Text,
        signature_verifier: impl SmartContractSignatureVerifier,
        signature_bytes: &[u8],
        account_id: AccountId,
        block_number: &mut Option<u64>,
    ) -> Result<Self, SignatureError> {
        // implements: IDENT-093
        let input = ScwInput::checked(
            account_id,
            alloy::primitives::eip191_hash_message(signature_text.as_ref()).into(),
            signature_bytes.to_vec().into(),
            *block_number,
        )?;
        let response = signature_verifier
            .is_valid_signature(
                input.account_id().clone(),
                input.hash(),
                input.signature().clone(),
                input.block_number(),
            )
            .await?;
        let verified = Self::from_scw_verdict(&input, &response, signature_text.as_ref())?;
        // set the block the signature was validated on
        *block_number = response.block_number;
        Ok(verified)
    }

    /// The signature a verifier judged valid for `input`: by the account's address, on the
    /// account's chain. An invalid verdict is [`SignatureError::Invalid`].
    pub(crate) fn from_scw_verdict(
        input: &ScwInput,
        response: &ValidationResponse,
        signature_text: &str,
    ) -> Result<Self, SignatureError> {
        if !response.is_valid {
            tracing::error!(
                "Smart contract wallet signature is invalid {:?}",
                response.error
            );
            return Err(SignatureError::Invalid);
        }
        Ok(Self::new_verified(
            MemberIdentifier::eth(input.account_id().get_account_address())?,
            SignatureKind::Erc1271,
            signature_text,
            Some(input.chain_id()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        InboxOwner,
        associations::{
            InstallationKeyContext, MemberIdentifier, SignatureKind,
            test_utils::{
                MockSmartContractSignatureVerifier, WalletTestExt, ecdsa_negated_s_alias,
                ecdsa_recovery_byte_alias, p256_negated_s_alias,
            },
            unverified::UnverifiedSignature,
            verified_signature::VerifiedSignature,
        },
        utils::passkey::PasskeyUser,
    };
    use alloy::signers::Signer;
    use alloy::signers::local::PrivateKeySigner;
    use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential};

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    // verifies: IDENT-030
    async fn test_recoverable_ecdsa() {
        let wallet = PrivateKeySigner::random();
        let signature_text = "test signature body";

        let signature = wallet
            .sign_message(signature_text.as_bytes())
            .await
            .unwrap();
        let verified_sig =
            VerifiedSignature::from_recoverable_ecdsa(signature_text, &signature.as_bytes())
                .expect("should succeed");

        assert_eq!(verified_sig.signer, wallet.member_identifier());
        assert_eq!(verified_sig.kind, SignatureKind::Erc191);
    }

    /// Every accepted encoding of one wallet signature (recovery byte 0/1 or 27/28, `s` in either
    /// half) recovers the same signer and yields one replay key. Any other recovery byte or
    /// length is rejected, so no further alias exists.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: IDENT-030, IDENT-090
    async fn identity_replay_aliases_share_a_wallet_key() {
        let wallet = PrivateKeySigner::random();
        let text = "replay alias";
        let signature = wallet.sign_message(text.as_bytes()).await?;
        let bytes = signature.as_bytes().to_vec();
        let negated = ecdsa_negated_s_alias(&bytes);
        let original = VerifiedSignature::from_recoverable_ecdsa(text, &bytes)?;

        for form in [
            ecdsa_recovery_byte_alias(&bytes),
            ecdsa_recovery_byte_alias(&negated),
            negated,
        ] {
            let verified = VerifiedSignature::from_recoverable_ecdsa(text, &form)?;
            assert_eq!(verified.signer, wallet.member_identifier());
            assert_eq!(verified.replay_key(), original.replay_key());
        }

        for v in [2, 26, 29, 35, 37, 255] {
            let form = [&bytes[..64], &[v]].concat();
            assert!(VerifiedSignature::from_recoverable_ecdsa(text, &form).is_err());
        }
        assert!(VerifiedSignature::from_recoverable_ecdsa(text, &bytes[..64]).is_err());
        assert!(
            VerifiedSignature::from_recoverable_ecdsa(text, &[bytes.as_slice(), &[0]].concat())
                .is_err()
        );
    }

    /// Both `s` forms of one passkey signature verify and yield one replay key.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: IDENT-030, IDENT-090
    async fn identity_replay_aliases_share_a_passkey_key() {
        let passkey = PasskeyUser::new().await;
        let text = "replay alias";
        let UnverifiedSignature::Passkey(signature) = passkey.sign(text)? else {
            unreachable!("a passkey signs with a passkey signature")
        };
        let verify = |der: &[u8]| {
            VerifiedSignature::from_passkey(
                text,
                &signature.public_key,
                der,
                &signature.authenticator_data,
                &signature.client_data_json,
            )
        };

        let original = verify(&signature.signature)?;
        let negated = verify(&p256_negated_s_alias(&signature.signature))?;
        assert_eq!(original.signer, negated.signer);
        assert_eq!(original.replay_key(), negated.replay_key());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    // verifies: IDENT-030
    async fn test_recoverable_ecdsa_incorrect() {
        let wallet = PrivateKeySigner::random();
        let signature_text = "test signature body";

        let sig_bytes: Vec<u8> = wallet
            .sign_message(signature_text.as_bytes())
            .await
            .unwrap()
            .into();

        let verified_sig =
            VerifiedSignature::from_recoverable_ecdsa("wrong text again", &sig_bytes).unwrap();
        assert_ne!(verified_sig.signer, wallet.member_identifier());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    // verifies: IDENT-030, IDENT-090
    async fn test_installation_key() {
        let key = XmtpInstallationCredential::new();
        let verifying_key = key.verifying_key();
        let signature_text = "test signature text";
        let sig = key
            .credential_sign::<InstallationKeyContext>(signature_text)
            .unwrap();

        let verified_sig =
            VerifiedSignature::from_installation_key(signature_text, sig.as_slice(), verifying_key)
                .expect("should succeed");
        let expected = MemberIdentifier::installation(verifying_key.as_bytes().to_vec());
        assert_eq!(expected, verified_sig.signer);
        assert_eq!(SignatureKind::InstallationKey, verified_sig.kind);
        // The replay key is the SHA-256 of the domain, the member kind, the signer's identifier
        // text, and the signature text, joined by line feeds; never the signature bytes.
        let preimage = format!(
            "XMTP identity replay key v1\ninstallation\n{}\n{signature_text}",
            hex::encode(verifying_key.as_bytes())
        );
        assert_eq!(verified_sig.replay_key(), sha256_bytes(preimage.as_bytes()));

        // Make sure it fails with the wrong signature text
        VerifiedSignature::from_installation_key(
            "wrong signature text",
            sig.as_slice(),
            verifying_key,
        )
        .expect_err("should fail with incorrect signature text");

        // Make sure it fails with the wrong verifying key
        VerifiedSignature::from_installation_key(
            signature_text,
            sig.as_slice(),
            XmtpInstallationCredential::new().verifying_key(),
        )
        .expect_err("should fail with incorrect verifying key");
    }

    /// A smart contract wallet signature has the replay key of an ERC-191 signature by the same
    /// address over the same text, whatever its bytes or chain.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    // verifies: IDENT-090
    async fn test_smart_contract_wallet() {
        let mock_verifier = MockSmartContractSignatureVerifier::new(true);
        let chain_id: u64 = 24;
        let wallet = PrivateKeySigner::random();
        let account_address = wallet.identifier().to_string();
        let account_id = AccountId::new(format!("eip155:{chain_id}"), account_address.clone());
        let signature_text = "test_smart_contract_wallet_signature";
        let signature_bytes = &[1, 2, 3];
        let mut block_number = Some(1);

        let verified_sig = VerifiedSignature::from_smart_contract_wallet(
            signature_text,
            mock_verifier,
            signature_bytes,
            account_id,
            &mut block_number,
        )
        .await
        .expect("should validate");
        assert_eq!(
            verified_sig.signer,
            MemberIdentifier::eth(account_address).unwrap()
        );
        assert_eq!(verified_sig.kind, SignatureKind::Erc1271);
        assert_eq!(verified_sig.chain_id, Some(chain_id));

        let erc191 = wallet
            .sign_message(signature_text.as_bytes())
            .await
            .unwrap();
        let erc191 = VerifiedSignature::from_recoverable_ecdsa(signature_text, &erc191.as_bytes())
            .expect("should succeed");
        assert_eq!(erc191.signer, verified_sig.signer);
        assert_eq!(erc191.replay_key(), verified_sig.replay_key());
    }
}
