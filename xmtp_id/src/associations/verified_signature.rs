#![allow(dead_code)]
use ethers::types::Signature as EthersSignature;
use ethers::utils::hash_message;
use xmtp_cryptography::signature::h160addr_to_string;
use xmtp_cryptography::CredentialVerify;

use crate::scw_verifier::SmartContractSignatureVerifier;

use super::{
    to_lower_s, AccountId, InstallationKeyContext, MemberIdentifier, SignatureError, SignatureKind,
};

#[derive(Debug, Clone)]
pub struct VerifiedSignature {
    pub signer: MemberIdentifier,
    pub kind: SignatureKind,
    pub raw_bytes: Vec<u8>,
    pub chain_id: Option<u64>,
}

impl VerifiedSignature {
    pub fn new(
        signer: MemberIdentifier,
        kind: SignatureKind,
        raw_bytes: Vec<u8>,
        chain_id: Option<u64>,
    ) -> Self {
        Self {
            signer,
            kind,
            raw_bytes,
            chain_id,
        }
    }

    /**
     * Verifies an ECDSA signature against the provided signature text.
     * Returns a VerifiedSignature if the signature is valid, otherwise returns an error.
     */
    pub fn from_recoverable_ecdsa<Text: AsRef<str>>(
        signature_text: Text,
        signature_bytes: &[u8],
    ) -> Result<Self, SignatureError> {
        let normalized_signature_bytes = to_lower_s(signature_bytes)?;
        let signature = EthersSignature::try_from(normalized_signature_bytes.as_slice())?;
        let address = h160addr_to_string(signature.recover(signature_text.as_ref())?);

        Ok(Self::new(
            MemberIdentifier::eth(address)?,
            SignatureKind::Erc191,
            normalized_signature_bytes.to_vec(),
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
            signature_text,
            signature_bytes.try_into()?,
        )?;
        Ok(Self::new(
            MemberIdentifier::installation(verifying_key.as_bytes().to_vec()),
            SignatureKind::InstallationKey,
            signature_bytes.to_vec(),
            None,
        ))
    }

    /// Verifies a smart contract wallet signature using the provided signature verifier.
    pub async fn from_smart_contract_wallet<Text: AsRef<str>>(
        signature_text: Text,
        signature_verifier: impl SmartContractSignatureVerifier,
        signature_bytes: &[u8],
        account_id: AccountId,
        block_number: &mut Option<u64>,
    ) -> Result<Self, SignatureError> {
        let response = signature_verifier
            .is_valid_signature(
                account_id.clone(),
                hash_message(signature_text.as_ref()).into(),
                signature_bytes.to_vec().into(),
                block_number.map(|n| n.into()),
            )
            .await?;

        if response.is_valid {
            // set the block the signature was validated on
            *block_number = response.block_number;

            Ok(Self::new(
                MemberIdentifier::eth(account_id.get_account_address())?,
                SignatureKind::Erc1271,
                signature_bytes.to_vec(),
                Some(account_id.get_chain_id_u64()?),
            ))
        } else {
            tracing::error!(
                "Smart contract wallet signature is invalid {:?}",
                response.error
            );
            Err(SignatureError::Invalid)
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::associations::{
        test_utils::{MockSmartContractSignatureVerifier, WalletTestExt},
        verified_signature::VerifiedSignature,
        InstallationKeyContext, MemberIdentifier, SignatureKind,
    };
    use ethers::signers::{LocalWallet, Signer};
    use xmtp_common::rand_hexstring;
    use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential};

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_recoverable_ecdsa() {
        let wallet: LocalWallet = LocalWallet::new(&mut rand::thread_rng());
        let signature_text = "test signature body";

        let sig_bytes: Vec<u8> = wallet.sign_message(signature_text).await.unwrap().to_vec();
        let verified_sig = VerifiedSignature::from_recoverable_ecdsa(signature_text, &sig_bytes)
            .expect("should succeed");

        assert_eq!(verified_sig.signer, wallet.member_identifier());
        assert_eq!(verified_sig.kind, SignatureKind::Erc191);
        assert_eq!(verified_sig.raw_bytes, sig_bytes);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_recoverable_ecdsa_incorrect() {
        let wallet: LocalWallet = LocalWallet::new(&mut rand::thread_rng());
        let signature_text = "test signature body";

        let sig_bytes: Vec<u8> = wallet.sign_message(signature_text).await.unwrap().to_vec();

        let verified_sig =
            VerifiedSignature::from_recoverable_ecdsa("wrong text again", &sig_bytes).unwrap();
        assert_ne!(verified_sig.signer, wallet.member_identifier());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
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
        assert_eq!(verified_sig.raw_bytes, sig.as_slice());

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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_smart_contract_wallet() {
        let mock_verifier = MockSmartContractSignatureVerifier::new(true);
        let chain_id: u64 = 24;
        let account_address = rand_hexstring();
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
        assert_eq!(verified_sig.raw_bytes, signature_bytes);
        assert_eq!(verified_sig.chain_id, Some(chain_id));
    }
}
